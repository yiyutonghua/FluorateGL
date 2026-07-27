//! SPIR-V 翻译管线编排模块
//!
//! 协调以下子模块完成桌面 GLSL → GLSL ES 的完整翻译：
//! 1. preprocess：GLSL 预处理（移除 #line、规范化版本、注入 location/binding）
//! 2. spirv_compile：GLSL → SPIR-V 编译（shaderc, Vulkan target）
//! 3. spirv_pass：SPIR-V 中间处理 Pass（预留扩展点，当前为直通）
//! 4. gles_compile：SPIR-V → GLSL ES 编译（spirv-cross2）
//! 5. postprocess：GLSL ES 后处理（移除 binding、处理 outColor、precision）

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
/// 无论 translate_internal 返回 Failed（shaderc 失败、空 SPIR-V、magic 校验失败）
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
            // shaderc 返回空 SPIR-V：喂给 spirv-cross 会触发 native segfault（空指针解引用）
            log::error!(
                "[ShaderTranslator] shaderc returned EMPTY SPIR-V for stage {} (0x{:04X}), took {:?}; source (first 500 chars):\n{}",
                stage_name,
                stage,
                compile_start.elapsed(),
                source.chars().take(500).collect::<String>()
            );
            return TranslationResult::Failed;
        }
        None => {
            log::warn!(
                "[ShaderTranslator] shaderc SPIR-V compile failed for stage {} (0x{:04X}), took {:?}; source (first 500 chars):\n{}",
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
        "[ShaderTranslator] shaderc SPIR-V compile done: stage={}, took {:?} ({} words)",
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
                    "[ShaderTranslator] SPIR-V translate success: stage={}, version=ES{}, shaderc={:?}, spirv-cross={:?}, total={:?}",
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

#[cfg(test)]
mod tests {
    use super::*;

    const GL_VERTEX_SHADER: u32 = 0x8B31;
    const GL_FRAGMENT_SHADER: u32 = 0x8B30;

    /// 断言翻译成功且产物是 GLSL ES（以 #version 3xx es 开头）
    fn assert_translated_to_gles(source: &str, stage: u32, name: &str) -> String {
        match translate(source, stage) {
            TranslationResult::Translated(src) => {
                assert!(
                    src.starts_with("#version 3"),
                    "{}: 翻译后应以 #version 3xx 开头，实际开头: {:?}",
                    name,
                    src.get(..20).unwrap_or("")
                );
                assert!(
                    src.contains("es"),
                    "{}: 翻译后应包含 es 标记，实际前 50 字符: {:?}",
                    name,
                    src.get(..50).unwrap_or("")
                );
                src
            }
            TranslationResult::PassThrough => panic!("{}: 不应透传", name),
            TranslationResult::Failed => panic!("{}: 翻译失败（不应返回 Failed）", name),
        }
    }

    #[test]
    fn test_simple_vertex_shader() {
        let src = r#"#version 330 core
in vec3 Position;
in vec4 Color;
out vec4 vertexColor;
void main() {
    gl_Position = vec4(Position, 1.0);
    vertexColor = Color;
}
"#;
        let translated = assert_translated_to_gles(src, GL_VERTEX_SHADER, "simple_vertex");
        // 翻译后应包含关键变量
        assert!(translated.contains("gl_Position"), "vertex shader 应保留 gl_Position");
        assert!(translated.contains("vertexColor"), "vertex shader 应保留 vertexColor");
    }

    #[test]
    fn test_simple_fragment_shader() {
        let src = r#"#version 330 core
in vec4 vertexColor;
out vec4 fragColor;
void main() {
    fragColor = vertexColor;
}
"#;
        let translated = assert_translated_to_gles(src, GL_FRAGMENT_SHADER, "simple_fragment");
        assert!(translated.contains("fragColor"), "fragment shader 应保留 fragColor");
    }

    #[test]
    fn test_ubo_shader() {
        // MC 风格的 UBO（无 layout(binding)，preprocess 会注入）
        let src = r#"#version 330
layout(std140) uniform DynamicTransforms {
    mat4 ModelViewMat;
    vec4 ColorModulator;
    vec3 ModelOffset;
};
in vec3 Position;
out vec4 vertexColor;
void main() {
    gl_Position = ModelViewMat * vec4(Position, 1.0);
    vertexColor = ColorModulator;
}
"#;
        let translated = assert_translated_to_gles(src, GL_VERTEX_SHADER, "ubo_shader");
        assert!(translated.contains("ModelViewMat"), "UBO 成员 ModelViewMat 应保留");
        assert!(translated.contains("ColorModulator"), "UBO 成员 ColorModulator 应保留");
    }

    #[test]
    fn test_sampler_texture() {
        let src = r#"#version 330 core
in vec2 vUV;
out vec4 fragColor;
uniform sampler2D Tex;
void main() {
    fragColor = texture(Tex, vUV);
}
"#;
        let translated = assert_translated_to_gles(src, GL_FRAGMENT_SHADER, "sampler_texture");
        assert!(
            translated.contains("texture(") || translated.contains("texture ("),
            "texture() 调用应保留"
        );
    }

    #[test]
    fn test_mc_style_with_line_directives() {
        // MC 的 moj_import 会产生 #line 指令，preprocess 应移除
        let src = r#"#version 150
#line 0
in vec3 Position;
#line 5
out vec4 vertexColor;
void main() {
    gl_Position = vec4(Position, 1.0);
    vertexColor = vec4(1.0);
}
"#;
        let translated = assert_translated_to_gles(src, GL_VERTEX_SHADER, "mc_line_directives");
        // #line 指令应在 preprocess 阶段被移除
        assert!(
            !translated.contains("#line"),
            "翻译后不应包含 #line 指令，实际: {:?}",
            translated
        );
    }

    #[test]
    fn test_mc_style_version_comment() {
        // MC 的 moj_import 会产生 /*#version N*/ 注释，preprocess 应移除
        let src = r#"/*#version 330*/
#version 150
in vec3 Position;
void main() {
    gl_Position = vec4(Position, 1.0);
}
"#;
        let translated = assert_translated_to_gles(src, GL_VERTEX_SHADER, "mc_version_comment");
        // /*#version*/ 注释应在 preprocess 阶段被移除
        assert!(
            !translated.contains("/*#version"),
            "翻译后不应包含 /*#version*/ 注释，实际: {:?}",
            translated
        );
    }

    #[test]
    fn test_glsl_150_default_version() {
        // MC 核心使用 #version 150，应能正确翻译
        let src = r#"#version 150
in vec3 Position;
in vec2 UV0;
out vec2 texCoord0;
void main() {
    gl_Position = vec4(Position, 1.0);
    texCoord0 = UV0;
}
"#;
        let translated = assert_translated_to_gles(src, GL_VERTEX_SHADER, "glsl_150");
        assert!(translated.contains("texCoord0"), "varying texCoord0 应保留");
    }

    #[test]
    fn test_discard_in_fragment() {
        let src = r#"#version 330 core
in vec4 vertexColor;
out vec4 fragColor;
void main() {
    if (vertexColor.a == 0.0) {
        discard;
    }
    fragColor = vertexColor;
}
"#;
        let translated = assert_translated_to_gles(src, GL_FRAGMENT_SHADER, "discard");
        assert!(translated.contains("discard"), "discard 应保留");
    }

    #[test]
    fn test_implicit_type_conversion() {
        // 桌面 GLSL 允许 ivec2 / float 隐式转换，GLSL ES 严格禁止
        // spirv-cross 应自动插入显式转换
        let src = r#"#version 330 core
uniform ivec2 offset;
out vec4 fragColor;
void main() {
    vec2 v = vec2(offset) * 0.5;
    fragColor = vec4(v, 0.0, 1.0);
}
"#;
        // 此测试主要验证不崩溃，类型转换由 spirv-cross 处理
        let _ = assert_translated_to_gles(src, GL_FRAGMENT_SHADER, "implicit_conversion");
    }
}
