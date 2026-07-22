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

use crate::shader_translator::{gles_compile, spirv_compile, string_pass};

#[derive(Debug, Clone)]
pub enum TranslationResult {
    Translated(String),
    PassThrough,
    Failed,
}

/// 翻译入口：将桌面 GLSL 翻译为 GLSL ES
///
/// 使用 catch_unwind 防止 panic 导致宿主进程崩溃。
///
/// **不变式**：此函数永不返回 `TranslationResult::Failed`。
/// 无论 translate_internal 返回 Failed（glslang 失败、空 SPIR-V、magic 校验失败）
/// 还是 panic（catch_unwind 捕获），都统一回退到 string_pass 字符串级翻译。
/// 这避免了 shader.rs 的 Failed 分支透传桌面 GLSL 给 GLES 导致崩溃。
pub fn translate(source: &str, stage: u32) -> TranslationResult {
    match std::panic::catch_unwind(|| translate_internal(source, stage)) {
        Ok(TranslationResult::Translated(s)) => TranslationResult::Translated(s),
        Ok(TranslationResult::PassThrough) => TranslationResult::PassThrough,
        Ok(TranslationResult::Failed) | Err(_) => {
            // 统一兜底：无论 SPIR-V 管线失败还是 panic，都走 string_pass
            log::warn!(
                "[ShaderTranslator] SPIR-V pipeline failed/panicked for stage 0x{:04X}, falling back to string_pass",
                stage
            );
            log::logger().flush();
            let fallback = string_pass::translate(source, stage);
            log::info!(
                "[ShaderTranslator] string_pass fallback produced {} chars for stage 0x{:04X}",
                fallback.len(),
                stage
            );
            TranslationResult::Translated(fallback)
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

    let total_start = std::time::Instant::now();

    // 步骤 1+2：预处理 + GLSL → SPIR-V
    let compile_start = std::time::Instant::now();
    let spv = match spirv_compile::compile(source, stage) {
        Some(s) if !s.is_empty() => s,
        Some(_) => {
            // glslang 返回空 SPIR-V：喂给 spirv-cross 会触发 native segfault（空指针解引用）
            log::error!(
                "[ShaderTranslator] glslang returned EMPTY SPIR-V for stage {} (0x{:04X}), took {:?}; source (first 500 chars):\n{}",
                stage_name,
                stage,
                compile_start.elapsed(),
                source.chars().take(500).collect::<String>()
            );
            return TranslationResult::Failed;
        }
        None => {
            log::warn!(
                "[ShaderTranslator] glslang SPIR-V compile failed for stage {} (0x{:04X}), took {:?}; source (first 500 chars):\n{}",
                stage_name,
                stage,
                compile_start.elapsed(),
                source.chars().take(500).collect::<String>()
            );
            return TranslationResult::Failed;
        }
    };
    // 验证 SPIR-V magic number，防止损坏的字节码喂给 spirv-cross 触发 native 崩溃
    const SPIRV_MAGIC: u32 = 0x07230203;
    if spv[0] != SPIRV_MAGIC {
        log::error!(
            "[ShaderTranslator] invalid SPIR-V magic number 0x{:08X} (expected 0x{:08X}) for stage {} (0x{:04X})",
            spv[0],
            SPIRV_MAGIC,
            stage_name,
            stage
        );
        return TranslationResult::Failed;
    }
    log::debug!(
        "[ShaderTranslator] glslang SPIR-V compile done: stage={}, took {:?} ({} words)",
        stage_name,
        compile_start.elapsed(),
        spv.len()
    );

    // 步骤 3：SPIR-V 中间处理 Pass（当前为直通，预留扩展点）
    let spv = spirv_pass(&spv);

    // 步骤 4+5：SPIR-V → GLSL ES + 后处理
    let cross_start = std::time::Instant::now();
    for gles_version in gles_compile::gles_version_candidates(source) {
        match gles_compile::compile(&spv, gles_version) {
            Ok(src) => {
                log::debug!(
                    "[ShaderTranslator] SPIR-V translate success: stage={}, version=ES{}, glslang={:?}, spirv-cross={:?}, total={:?}",
                    stage_name,
                    gles_version,
                    compile_start.elapsed(),
                    cross_start.elapsed(),
                    total_start.elapsed()
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
        "[ShaderTranslator] all GLES versions failed for shader stage {}, falling back to string_pass; total took {:?}",
        stage_name,
        total_start.elapsed()
    );
    // 回退到字符串级翻译（string_pass），而非直接透传桌面 GLSL 给 GLES（几乎必然编译失败）。
    // string_pass 做版本替换、legacy 语法迁移、precision 注入等，作为 SPIR-V 管线失败的兜底。
    let fallback = string_pass::translate(source, stage);
    log::info!(
        "[ShaderTranslator] string_pass fallback produced {} chars for stage {} (0x{:04X})",
        fallback.len(),
        stage_name,
        stage
    );
    TranslationResult::Translated(fallback)
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
