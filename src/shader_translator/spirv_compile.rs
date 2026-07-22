//! GLSL → SPIR-V 编译模块
//!
//! 使用 glslang crate 将桌面 GLSL 编译为 SPIR-V 字节码。
//! 对齐 MobileGlues 的 glslang 配置：
//! - client = OpenGL（EShClientOpenGL + EShTargetOpenGL_450）
//! - target_language = SPIR-V 1.5（EShTargetSpv + EShTargetSpv_1_5）
//! - ShaderOptions: AUTO_MAP_BINDINGS | AUTO_MAP_LOCATIONS | VULKAN_RULES_RELAXED
//!
//! 注意：MobileGlues 的 C++ API 通过 setEnvInput(EShClientVulkan) + setEnvClient(EShClientOpenGL)
//! 实现"Vulkan 输入 + OpenGL 客户端"的混合模式。Rust crate 的 glslang_input_t 只有一个
//! client 字段，但 setEnvClient 会覆盖 setEnvInput 设置的 client，所以最终效果等价于
//! 纯 OpenGL 客户端 + SPIR-V 目标。这里使用 Target::OpenGL { spirv_version: Some(...) }。
//!
//! 相比 Target::Vulkan，OpenGL SPIR-V 模式更宽松：
//! - 允许独立 non-opaque uniform（无需包装进 UBO block）
//! - 允许省略 layout(location)（由 AUTO_MAP_LOCATIONS 自动分配）
//! - 允许省略 layout(binding)（由 AUTO_MAP_BINDINGS 自动分配）
//! - 要求 GLSL >= 330（Vulkan 仅要求 >= 140）

use glslang::{
    Compiler, CompilerOptions, OpenGlVersion, ShaderInput, ShaderMessage, ShaderOptions,
    ShaderSource, ShaderStage, SourceLanguage, SpirvVersion, Target,
};

// GL shader stage 常量
pub const GL_VERTEX_SHADER: u32 = 0x8B31;
pub const GL_FRAGMENT_SHADER: u32 = 0x8B30;
pub const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
pub const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
pub const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;
pub const GL_COMPUTE_SHADER: u32 = 0x91B9;

/// 将 GLSL 源码编译为 SPIR-V 字节码
///
/// 流程：预处理 → glslang parse → glslang compile → SPIR-V
/// 失败时返回 None 并输出 error 级别日志。
pub fn compile(source: &str, stage: u32) -> Option<Vec<u32>> {
    let compiler = match Compiler::acquire() {
        Some(c) => c,
        None => {
            log::error!(
                "[ShaderTranslator] glslang compiler not available (glslang_initialize_process failed)"
            );
            return None;
        }
    };
    let glsl_stage = match map_gl_stage(stage) {
        Some(s) => s,
        None => {
            log::error!(
                "[ShaderTranslator] map_gl_stage returned None for stage 0x{:04X}; source (first 500 chars):\n{}",
                stage,
                source.chars().take(500).collect::<String>()
            );
            return None;
        }
    };

    // 预处理 GLSL：移除 #line、强制版本 >= 150、补全 location/binding
    let preprocessed = crate::shader_translator::preprocess::preprocess(source);

    let src = ShaderSource::from(preprocessed.as_str());

    // 对齐 MobileGlues: OpenGL 客户端 + SPIR-V 1.5 目标
    // EShClientOpenGL + EShTargetOpenGL_450 + EShTargetSpv + EShTargetSpv_1_5
    // 相比 Vulkan 目标，OpenGL SPIR-V 允许独立 uniform、省略 location/binding
    let options = CompilerOptions {
        source_language: SourceLanguage::GLSL,
        target: Target::OpenGL {
            version: OpenGlVersion::OpenGL4_5,
            spirv_version: Some(SpirvVersion::SPIRV1_5),
        },
        version_profile: None,
        messages: ShaderMessage::SUPPRESS_WARNINGS,
    };

    let input = match ShaderInput::new(
        &src,
        glsl_stage,
        &options,
        None::<&[(&str, Option<&str>)]>,
        None,
    ) {
        Ok(input) => input,
        Err(e) => {
            log::error!(
                "[ShaderTranslator] glslang parse failed for stage 0x{:04X}: {:?}; source (first 500 chars):\n{}",
                stage,
                e,
                source.chars().take(500).collect::<String>()
            );
            return None;
        }
    };

    let mut shader = match compiler.create_shader(input) {
        Ok(shader) => shader,
        Err(e) => {
            log::error!(
                "[ShaderTranslator] glslang shader creation failed for stage 0x{:04X}: {:?}; source (first 500 chars):\n{}",
                stage,
                e,
                source.chars().take(500).collect::<String>()
            );
            return None;
        }
    };

    // 对齐 MobileGlues: 开启 AutoMapBindings + AutoMapLocations + VulkanRulesRelaxed
    shader.options(
        ShaderOptions::AUTO_MAP_BINDINGS
            | ShaderOptions::AUTO_MAP_LOCATIONS
            | ShaderOptions::VULKAN_RULES_RELAXED,
    );

    match shader.compile() {
        Ok(spv) => Some(spv),
        Err(e) => {
            log::error!(
                "[ShaderTranslator] glslang SPIR-V compile failed for stage 0x{:04X}: {:?}; source (first 500 chars):\n{}",
                stage,
                e,
                source.chars().take(500).collect::<String>()
            );
            None
        }
    }
}

/// GL stage 常量 → glslang ShaderStage 映射
pub fn map_gl_stage(stage: u32) -> Option<ShaderStage> {
    match stage {
        GL_VERTEX_SHADER => Some(ShaderStage::Vertex),
        GL_FRAGMENT_SHADER => Some(ShaderStage::Fragment),
        GL_GEOMETRY_SHADER => Some(ShaderStage::Geometry),
        GL_TESS_CONTROL_SHADER => Some(ShaderStage::TesselationControl),
        GL_TESS_EVALUATION_SHADER => Some(ShaderStage::TesselationEvaluation),
        GL_COMPUTE_SHADER => Some(ShaderStage::Compute),
        _ => None,
    }
}

/// stage 常量 → 可读名称（用于日志）
pub fn stage_name(stage: u32) -> &'static str {
    match stage {
        GL_VERTEX_SHADER => "vertex",
        GL_FRAGMENT_SHADER => "fragment",
        GL_GEOMETRY_SHADER => "geometry",
        GL_TESS_CONTROL_SHADER => "tess_control",
        GL_TESS_EVALUATION_SHADER => "tess_eval",
        GL_COMPUTE_SHADER => "compute",
        _ => "unknown",
    }
}
