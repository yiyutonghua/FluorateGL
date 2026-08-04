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
//! - `Error::CompilationError(String)` 直接携带 glslang 完整诊断信息
//!
//! ## 编译流程
//!
//! 1. preprocess：移除 #line、移除 /*#version*/ 注释、统一版本 450 core、
//!    迁移 attribute/varying 老语法、注入 location/binding
//! 2. shaderc compile_into_spirv：GLSL → SPIR-V（target=OpenGL, env=450, SPIRV1_5）
//!
//! OpenGL target 要求（spike 实测）：桌面 GLSL >= 330，所有 in/out 有 location，
//! UBO/SSBO 有 binding，non-opaque standalone uniform 有 location。
//! 与 Vulkan target 不同：standalone uniform 合法（无需 UBO 包装）、
//! gl_VertexID 保留原名、glslang 不定义 VULKAN 宏。
//! preprocess 负责注入这些。

use shaderc::{CompileOptions, Compiler, OptimizationLevel, ShaderKind, SpirvVersion};
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

    // 预处理 GLSL：移除 #line、移除 /*#version*/ 注释、统一版本 450 core、
    // 迁移 attribute/varying、注入 location/binding
    // stage 用于 attribute/varying → in/out 的关键字迁移（VS: attribute→in,
    // varying→out；FS: varying→in）
    let preprocessed = crate::shader_translator::preprocess::preprocess(source, stage);

    // 诊断：记录原始 shader 源码前 300 字符，确认 VULKAN 条件编译分支走向
    // （OpenGL target 下 glslang 不定义 VULKAN 宏，spike_h 实测）
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
    // - target: OpenGL 450（glslang OpenGL 语义模式，spike 实测：standalone
    //   uniform 合法只需 location、gl_VertexID 保留原名、不定义 VULKAN 宏）
    // - optimization: Zero（保留变量名/OpName，避免优化消除名称导致 uniform 查找失败）
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

    // target env: OpenGL（版本 450 = 桌面 GLSL 版本号，shaderc 接受裸 u32，见 spike）。
    // OpenGL target 要求桌面 GLSL >= 330；preprocess 已统一升级到 450 core
    // （uniform location 注入需 430+，binding 注入需 420+，450 全覆盖）。
    options.set_target_env(shaderc::TargetEnv::OpenGL, 450);
    // 优化级别：Zero（不做 SPIRV-Tools 优化，保留变量名/OpName/OpMemberName）。
    // 历史教训：Performance 级别的 aggressive-dce 可能消除未使用变量及其
    // OpName/OpMemberName，导致 spirv-cross 输出 fallback 名（如 _13），
    // MC 的 glGetUniformLocation 按变量名查找 uniform 会失败。
    // 稳妥优先，回退 Zero（shader 编译只在加载时发生一次，性能影响可接受）。
    options.set_optimization_level(OptimizationLevel::Zero);
    // 生成 debug info：确保 OpName/OpMemberName/OpLine/OpSource 等诊断指令保留。
    // spirv-cross 依赖 OpName 还原变量名（如 sampler `Tex`、UBO 成员 `ModelViewMat`）。
    options.set_generate_debug_info();
    // 自动给没有显式 binding 的 uniform（包括 sampler）分配 binding point。
    // OpenGL target 下 sampler 无 binding 也能编译（spike_c 实测，glslang 自动
    // 分配 binding=0），此选项保持显式分配，行为确定、无害。
    options.set_auto_bind_uniforms(true);
    // 启用 SPIR-V 1.5（spike 实测 OpenGL env 下显式设置 V1_5 编译成功）
    options.set_target_spirv(SpirvVersion::V1_5);

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
