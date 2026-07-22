//! GLSL → SPIR-V 编译模块
//!
//! 使用 glslang crate 将桌面 GLSL 编译为 SPIR-V 字节码。
//! 本分支（glslang-targetvk）实验性使用 Vulkan target：
//! - target = Vulkan 1.2 + SPIR-V 1.5
//! - ShaderOptions: AUTO_MAP_BINDINGS | AUTO_MAP_LOCATIONS | VULKAN_RULES_RELAXED
//!
//! 背景：glslang 0.8.1 的 Program::compile 硬编码了 VULKAN_RULES | SPV_RULES，
//! 即使 target 是 OpenGL，compile 阶段也按 Vulkan 规则校验。使用 Target::Vulkan
//! 使 client/target 标记与实际校验行为一致，避免 OpenGL target 下 parse 阶段
//! 与 compile 阶段规则不一致的问题。
//!
//! Vulkan target 要求：
//! - GLSL >= 140（preprocess 已升级到 >= 330，满足）
//! - 所有 in/out 有 location（preprocess 已注入，满足）
//! - 所有 UBO/SSBO 有 binding（preprocess 已注入，满足）
//! - 独立 non-opaque uniform 需包装进 UBO（VULKAN_RULES_RELAXED 可能放宽，待验证）

use glslang::{
    Compiler, CompilerOptions, ShaderInput, ShaderMessage, ShaderOptions,
    ShaderSource, ShaderStage, SourceLanguage, SpirvVersion, Target, VulkanVersion,
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

    // 本分支实验性配置：Vulkan target + SPIR-V 1.5
    // EShClientVulkan + EShTargetVulkan_1_2 + EShTargetSpv + EShTargetSpv_1_5
    // glslang Program::compile 硬编码 VULKAN_RULES，用 Vulkan target 使标记一致
    let options = CompilerOptions {
        source_language: SourceLanguage::GLSL,
        target: Target::Vulkan {
            version: VulkanVersion::Vulkan1_2,
            spirv_version: SpirvVersion::SPIRV1_5,
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

    // glslang compile 是 FFI 调用（C++ 代码），native 崩溃（segfault/SIGABRT）
    // 无法被 catch_unwind 捕获。此处用 info 级日志 + flush 标记进入/退出，
    // 若崩溃后日志只有 "ENTERING" 无 "EXITED"，则确认崩溃在 glslang 内部。
    log::info!(
        "[ShaderTranslator] ENTERING glslang compile for stage 0x{:04X} (source {} chars, preprocessed {} chars)",
        stage,
        source.len(),
        preprocessed.len()
    );
    log::logger().flush();
    match shader.compile() {
        Ok(spv) => {
            log::info!(
                "[ShaderTranslator] EXITED glslang compile OK for stage 0x{:04X} (SPIR-V {} words)",
                stage,
                spv.len()
            );
            log::logger().flush();
            Some(spv)
        }
        Err(e) => {
            log::error!(
                "[ShaderTranslator] EXITED glslang compile FAILED for stage 0x{:04X}: {:?}; source (first 500 chars):\n{}",
                stage,
                e,
                source.chars().take(500).collect::<String>()
            );
            log::logger().flush();
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
