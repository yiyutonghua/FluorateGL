//! FluorateGL 集成测试。
//!
//! 这里只校验不依赖 GLES 后端的纯 Rust 行为：
//! 1. 公共模块能被外部 crate 引用；
//! 2. `shader_translator::spirv_pass::translate` 在桌面 GLSL 330 输入下能产出 GLSL ES 输出；
//! 3. 不应 panic / 不需要 EGL 上下文。
//!
//! 依赖 GLES 后端的端到端测试见 `tests/gl/test_shader_translation.c`（通过 `tests/run.sh` 调用）。

use fluorategl::shader_translator::spirv_pass::{TranslationResult, translate};

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;

#[test]
fn translate_vertex_330_produces_gles_es() {
    let src = "#version 330 core\n\
               uniform mat4 MVP;\n\
               in vec3 Position;\n\
               void main() { gl_Position = MVP * vec4(Position, 1.0); }\n";
    let result = translate(src, GL_VERTEX_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            assert!(out.contains("#version"), "missing #version: {}", out);
            assert!(!out.contains("binding = 0) uniform mat4"), "got: {}", out);
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}

#[test]
fn translate_fragment_with_sampler_succeeds() {
    let src = "#version 330 core\n\
               uniform sampler2D tex;\n\
               in vec2 uv;\n\
               out vec4 fragColor;\n\
               void main() { fragColor = texture(tex, uv); }\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            assert!(out.contains("texture(tex, uv)"), "got: {}", out);
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}

#[test]
fn translate_legacy_150_vertex_fails_gracefully() {
    // shaderc 要求 SPIR-V 输入至少 330，150 应返回 Failed 而非 panic。
    let src = "#version 150 core\nvoid main() {}\n";
    let result = translate(src, GL_VERTEX_SHADER);
    assert!(
        matches!(result, TranslationResult::Failed),
        "expected Failed for legacy #version 150, got {:?}",
        result
    );
}

#[test]
fn translate_invalid_source_does_not_panic() {
    let src = "#version 330 core\nthis is not valid glsl\n";
    let result = translate(src, GL_VERTEX_SHADER);
    assert!(matches!(result, TranslationResult::Failed), "got {:?}", result);
}
