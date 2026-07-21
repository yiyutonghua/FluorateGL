//! SPIR-V 翻译管线编排模块
//!
//! 协调以下子模块完成桌面 GLSL → GLSL ES 的完整翻译：
//! 1. preprocess：GLSL 预处理（移除 #line、版本升级、注入 location/binding）
//! 2. spirv_compile：GLSL → SPIR-V 编译（glslang）
//! 3. spirv_pass：SPIR-V 中间处理 Pass（预留扩展点，当前为直通）
//! 4. gles_compile：SPIR-V → GLSL ES 编译（spirv-cross2）
//! 5. postprocess：GLSL ES 后处理（移除 binding、处理 outColor、precision）
//!
//! 对齐 MobileGlues 的 translate_glsl_to_glsles 流程。

use crate::shader_translator::{gles_compile, spirv_compile};

#[derive(Debug, Clone)]
pub enum TranslationResult {
    Translated(String),
    PassThrough,
    Failed,
}

/// 翻译入口：将桌面 GLSL 翻译为 GLSL ES
///
/// 使用 catch_unwind 防止 panic 导致宿主进程崩溃。
pub fn translate(source: &str, stage: u32) -> TranslationResult {
    match std::panic::catch_unwind(|| translate_internal(source, stage)) {
        Ok(result) => result,
        Err(_) => {
            log::error!(
                "[ShaderTranslator] SPIR-V translation panicked for stage 0x{:04X}; skipping",
                stage
            );
            TranslationResult::Failed
        }
    }
}

/// 翻译管线内部实现
///
/// 流程：预处理 → GLSL→SPIR-V → SPIR-V Pass → SPIR-V→GLSL ES（含后处理）
fn translate_internal(source: &str, stage: u32) -> TranslationResult {
    let stage_name = spirv_compile::stage_name(stage);
    log::debug!(
        "[ShaderTranslator] SPIR-V translate start: stage={} (0x{:04X})",
        stage_name,
        stage
    );

    // 步骤 1+2：预处理 + GLSL → SPIR-V
    let spv = match spirv_compile::compile(source, stage) {
        Some(s) => s,
        None => {
            log::warn!(
                "[ShaderTranslator] glslang SPIR-V compile failed for stage {} (0x{:04X})",
                stage_name,
                stage
            );
            return TranslationResult::Failed;
        }
    };

    // 步骤 3：SPIR-V 中间处理 Pass（当前为直通，预留扩展点）
    let spv = spirv_pass(&spv);

    // 步骤 4+5：SPIR-V → GLSL ES + 后处理
    for gles_version in gles_compile::gles_version_candidates(source) {
        match gles_compile::compile(&spv, gles_version) {
            Ok(src) => {
                log::debug!(
                    "[ShaderTranslator] SPIR-V translate success: stage={}, version=ES{}",
                    stage_name,
                    gles_version
                );
                log::debug!("[ShaderTranslator] translated GLSL ES:\n{}", src);
                return TranslationResult::Translated(src);
            }
            Err(e) => {
                log::warn!(
                    "[ShaderTranslator] GLES ES{} write failed for stage {}: {:?}",
                    gles_version,
                    stage_name,
                    e
                );
            }
        }
    }

    log::warn!(
        "[ShaderTranslator] all GLES versions failed for shader stage {}",
        stage_name
    );
    TranslationResult::Failed
}

/// SPIR-V 中间处理 Pass
///
/// 当前为直通（返回原始 SPIR-V），预留扩展点用于：
/// - SPIR-V 优化（spirv-tools）
/// - 精度修正 Pass
/// - 插值修饰符 Pass
/// - 位置不变性 Pass
///
/// 对齐 MobileGL 的 SpirvPasses 体系，但当前 FluorateGL 不需要这些复杂处理。
fn spirv_pass(spv: &[u32]) -> Vec<u32> {
    spv.to_vec()
}
