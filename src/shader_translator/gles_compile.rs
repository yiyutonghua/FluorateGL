//! SPIR-V → GLSL ES 编译模块
//!
//! 使用 spirv-cross2 crate 将 SPIR-V 字节码反编译为 GLSL ES。
//! 对齐 MobileGlues 的 spirv-cross 配置：
//! - es_default_float_precision_highp = true
//! - es_default_int_precision_highp = true
//! - common.flip_vertex_y = true
//! - common.fixup_clipspace = true
//! - common.emit_line_directives = false

use spirv_cross2::compile::glsl::GlslVersion;
use spirv_cross2::compile::{CompilableTarget, CompiledArtifact};
use spirv_cross2::targets::Glsl;
use spirv_cross2::{Compiler as SpvCompiler, Module, SpirvCrossError};

use crate::shader_translator::postprocess;
use crate::shader_translator::preprocess;

/// 将 SPIR-V 字节码编译为 GLSL ES 源码
///
/// 流程：SPIR-V → spirv-cross 编译 → 后处理（移除 binding、处理 outColor、precision）
/// 失败时返回 Err 并携带错误信息。
pub fn compile(spv: &[u32], version: u16) -> Result<String, SpirvCrossError> {
    // 空 SPIR-V 防护：Module::from_words(&[]) + SpvCompiler::new 会触发 native segfault
    if spv.is_empty() {
        log::error!(
            "[ShaderTranslator] spirv-cross received EMPTY SPIR-V for ES{}",
            version
        );
        return Err(SpirvCrossError::InvalidSpirv(
            "empty SPIR-V module".to_string(),
        ));
    }

    let module = Module::from_words(spv);
    let compiler = SpvCompiler::<Glsl>::new(module)?;
    let mut options = Glsl::options();

    options.version = match version {
        320 => GlslVersion::Glsl320Es,
        310 => GlslVersion::Glsl310Es,
        300 => GlslVersion::Glsl300Es,
        _ => GlslVersion::Glsl300Es,
    };

    // spirv-cross2 默认 es_default_float_precision_highp = false（mediump），
    // MC shader 需要 highp 避免精度不足
    options.es_default_float_precision_highp = true;
    options.es_default_int_precision_highp = true;

    // 不翻转 Y 轴：输入是桌面 OpenGL SPIR-V（client=OpenGL，NDC Y-up），
    // GLES 同样是 Y-up，无需翻转。MobileGlues 设 true 是因其输入方言为 Vulkan（Y-down），
    // FluorateGL 用 Target::OpenGL（Y-up），设 true 会导致 Y 二次翻转、画面上下颠倒。
    options.common.flip_vertex_y = false;

    // 不修正 clip space：桌面 OpenGL 与 GLES 都使用 [-w,w] clip space（NDC 深度 [-1,1]），
    // 无需 remap。fixup_clipspace 假设输入是 Vulkan（[0,w]），设 true 会把 [-w,w] 错误映射，
    // 导致深度测试全错。MobileGlues 设 true 同样是因 Vulkan 输入方言。
    options.common.fixup_clipspace = false;

    // 不输出 #line 指令（我们在后处理中统一清理）
    options.common.emit_line_directives = false;

    // spirv-cross compile 是 FFI 调用（C++ 代码），native 崩溃无法被 catch_unwind 捕获。
    // debug 级日志标记进入/退出，便于按需开启精确定位是否 spirv-cross 崩溃；
    // 默认 debug 关闭，避免每个 shader 翻译都刷屏。
    // log::Logger 已在每条日志后 flush（见 util/log.rs），开启 debug 时仍能落盘。
    log::debug!(
        "[ShaderTranslator] ENTERING spirv-cross compile for ES{} (SPIR-V {} words)",
        version,
        spv.len()
    );
    let artifact: CompiledArtifact<Glsl> = compiler.compile(&options)?;
    let src = artifact.to_string();
    log::debug!(
        "[ShaderTranslator] EXITED spirv-cross compile OK for ES{} ({} chars)",
        version,
        src.len()
    );

    // 后处理：移除 binding、处理 outColor location、确保 precision
    Ok(postprocess::post_process(&src))
}

/// 根据原始桌面 GLSL 版本推导候选 GLES 版本列表
///
/// 策略：高版本桌面 GLSL → 尝试 GLES 320/310/300，否则尝试 310/300
pub fn gles_version_candidates(source: &str) -> Vec<u16> {
    let desktop_version = preprocess::extract_version(source)
        .and_then(|line| {
            line.split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<u32>().ok())
        })
        .unwrap_or(150);
    match desktop_version {
        460 | 450 | 440 | 430 | 420 | 410 | 400 | 330 => vec![320, 310, 300],
        _ => vec![310, 300],
    }
}
