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

    // GLES 坐标系 Y 轴翻转（对齐 MobileGlues）
    options.common.flip_vertex_y = true;

    // Vulkan [0,w] 深度范围 → OpenGL [-w,w] 深度范围（对齐 MobileGlues）
    options.common.fixup_clipspace = true;

    // 不输出 #line 指令（我们在后处理中统一清理）
    options.common.emit_line_directives = false;

    let artifact: CompiledArtifact<Glsl> = compiler.compile(&options)?;
    let src = artifact.to_string();

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
