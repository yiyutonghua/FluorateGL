//! GLSL → SPIR-V 编译模块
//!
//! 使用 shaderc（Google 维护的 glslang 封装层）将桌面 GLSL 编译为 SPIR-V 字节码。
//!
//! ## 为什么用 shaderc 而非直接 FFI 调用 glslang_sys
//!
//! 之前直接调用 glslang_sys C API（方案C），在 parse 阶段对大 fragment shader
//! 触发崩溃，且无法被 catch_unwind 捕获、关键诊断日志被日志后端吞掉。
//!
//! shaderc 的优势：
//! - 高层 safe Rust API，无需手动管理 shader/program 生命周期
//! - 内部用 C++ try/catch 兜底，即使底层 glslang 崩溃也返回 `Error` 而非 native crash
//! - 默认 target 为 Vulkan，自动处理 VULKAN_RULES_RELAXED 等规则
//! - `Error::CompilationError(String)` 直接携带 glslang 完整诊断信息
//!
//! ## 编译流程
//!
//! 1. preprocess：移除 #line、移除 /*#version*/ 注释、规范化版本、注入 location/binding
//! 2. shaderc compile_into_spirv：GLSL → SPIR-V（target=Vulkan1_2, SPIRV1_5）
//!
//! Vulkan target 要求：GLSL >= 140，所有 in/out 有 location，UBO/SSBO 有 binding。
//! preprocess 负责注入这些。

use shaderc::{CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, TargetEnv};
use std::sync::OnceLock;

// GL shader stage 常量
pub const GL_VERTEX_SHADER: u32 = 0x8B31;
pub const GL_FRAGMENT_SHADER: u32 = 0x8B30;
pub const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
pub const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
pub const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;
pub const GL_COMPUTE_SHADER: u32 = 0x91B9;

/// 全局 Compiler 单例
///
/// shaderc::Compiler 构造代价高（内部初始化 glslang 全局状态），
/// 用 OnceLock 缓存，整个进程生命周期复用一个实例。
/// Compiler 是 Send+Sync，可安全跨线程使用。
static COMPILER: OnceLock<Option<Compiler>> = OnceLock::new();

/// 获取全局 Compiler 实例
///
/// 返回 `Option<&Compiler>`：
/// - `Some(&compiler)`：编译器初始化成功
/// - `None`：初始化失败（glslang_initialize_process 失败，通常表示系统问题）
fn get_compiler() -> Option<&'static Compiler> {
    COMPILER
        .get_or_init(|| match Compiler::new() {
            Ok(c) => {
                log::info!("[ShaderTranslator] shaderc Compiler initialized");
                Some(c)
            }
            Err(e) => {
                log::error!("[ShaderTranslator] shaderc Compiler::new() failed: {:?}", e);
                None
            }
        })
        .as_ref()
}

/// 将 GLSL 源码编译为 SPIR-V 字节码
///
/// 流程：preprocess（注入 location/binding）→ shaderc compile_into_spirv
///
/// 失败时返回 None 并输出 error 级别日志（含 glslang 诊断信息）。
pub fn compile(source: &str, stage: u32) -> Option<Vec<u32>> {
    let compiler = match get_compiler() {
        Some(c) => c,
        None => {
            log::error!("[ShaderTranslator] shaderc compiler not available");
            return None;
        }
    };

    let kind = match map_gl_stage(stage) {
        Some(k) => k,
        None => {
            log::error!(
                "[ShaderTranslator] map_gl_stage returned None for stage 0x{:04X}",
                stage
            );
            return None;
        }
    };

    // 预处理 GLSL：移除 #line、移除 /*#version*/ 注释、规范化版本、注入 location/binding
    // stage 用于给 UniformBlock 起唯一块名（UniformBlockVS/UniformBlockFS），避免跨 stage type mismatch
    let preprocessed = crate::shader_translator::preprocess::preprocess(source, stage);

    // 诊断：记录原始 shader 源码前 300 字符，确认是否有 VULKAN 条件编译
    // 以及 #undef VULKAN 是否生效（preprocessed 中会包含 #undef VULKAN）
    log::debug!(
        "[ShaderTranslator] ENTERING shaderc compile for stage 0x{:04X} (source {} chars, preprocessed {} chars)",
        stage,
        source.len(),
        preprocessed.len()
    );
    log::debug!(
        "[ShaderTranslator] original source (first 300 chars):\n{}",
        source.chars().take(300).collect::<String>()
    );

    // 构造编译选项
    // - target: Vulkan 1.2（支持现代 SPIR-V 特性）
    // - optimization: Performance（启用 SPIRV-Tools 优化）
    let mut options = match CompileOptions::new() {
        Ok(o) => o,
        Err(e) => {
            log::error!(
                "[ShaderTranslator] shaderc CompileOptions::new() failed: {:?}",
                e
            );
            return None;
        }
    };
    options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_2 as u32);
    // 优化级别：Performance（启用 SPIRV-Tools 优化，提高性能）
    // 之前使用 Zero 级别是为了保留变量名，但现代 shaderc 和 spirv-cross 能更好地处理
    // debug info，Performance 级别不会破坏变量名映射，同时提高编译性能
    options.set_optimization_level(OptimizationLevel::Performance);
    // 生成 debug info：确保 OpName/OpMemberName/OpLine/OpSource 等诊断指令保留。
    // spirv-cross 依赖 OpName 还原变量名（如 sampler `Tex`、UBO 成员 `ModelViewMat`）。
    options.set_generate_debug_info();
    // 自动给没有显式 binding 的 uniform（包括 sampler）分配 binding point。
    // Vulkan target 要求所有 opaque uniform（sampler/image/UBO/SSBO）必须有 binding。
    // 桌面 GLSL 允许省略 binding（由链接器分配），preprocess 已给 UBO/SSBO 注入 binding，
    // 但独立 sampler（如 `uniform sampler2D Tex;`）仍缺 binding，用此选项自动分配。
    // 不会影响已有显式 binding 的 uniform。
    options.set_auto_bind_uniforms(true);
    // 启用 SPIR-V 1.5（支持更多现代特性）
    options.set_target_spirv_version((1, 5));
    // 启用 Vulkan 规则放宽（支持更多 GLSL 特性）
    options.set_vulkan_rules_relaxed(true);
    // 启用分离着色器（提高编译效率）
    options.set_separate_shader_objects(true);

    // 执行编译
    let result = compiler.compile_into_spirv(
        &preprocessed,
        kind,
        "shader.glsl", // 用于诊断的文件名
        "main",        // 入口点（GLSL 默认 main）
        Some(&options),
    );

    match result {
        Ok(artifact) => {
            let spv: Vec<u32> = artifact.as_binary().to_vec();
            log::debug!(
                "[ShaderTranslator] EXITED shaderc compile OK for stage 0x{:04X} (SPIR-V {} words)",
                stage,
                spv.len()
            );
            Some(spv)
        }
        Err(shaderc::Error::CompilationError(code, msg)) => {
            log::error!(
                "[ShaderTranslator] shaderc compile FAILED for stage 0x{:04X} (code {}): {}; source (first 500 chars):\n{}",
                stage,
                code,
                msg,
                source.chars().take(500).collect::<String>()
            );
            None
        }
        Err(e) => {
            log::error!(
                "[ShaderTranslator] shaderc compile error for stage 0x{:04X}: {:?}",
                stage,
                e
            );
            None
        }
    }
}

/// GL stage 常量 → shaderc::ShaderKind 映射
pub fn map_gl_stage(stage: u32) -> Option<ShaderKind> {
    match stage {
        GL_VERTEX_SHADER => Some(ShaderKind::Vertex),
        GL_FRAGMENT_SHADER => Some(ShaderKind::Fragment),
        GL_GEOMETRY_SHADER => Some(ShaderKind::Geometry),
        GL_TESS_CONTROL_SHADER => Some(ShaderKind::TessControl),
        GL_TESS_EVALUATION_SHADER => Some(ShaderKind::TessEvaluation),
        GL_COMPUTE_SHADER => Some(ShaderKind::Compute),
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
