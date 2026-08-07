//! spirv_pass 翻译管线端到端测试
//!
//! 覆盖 `spirv_pass::translate` 的完整管线：
//! 预处理 → GLSL→SPIR-V → SPIR-V Pass → SPIR-V→GLSL ES → 后处理
//!
//! 测试场景：
//! - 各种 vertex/fragment/compute shader 端到端翻译
//! - Minecraft 风格 shader（UBO + 多 in/out 变量）
//! - 非法 GLSL 优雅失败（返回 Failed 而非 panic）
//! - legacy 版本（< 330）经 preprocess 升级后编译成功
//! - 无效 stage 返回 Failed
//! - TranslationResult 枚举变体验证
//! - 翻译输出包含 GLSL ES 版本和 precision

use fluorategl::shader_translator::spirv_compile;
use fluorategl::shader_translator::spirv_pass::{TranslationResult, translate};

const GL_VERTEX_SHADER: u32 = spirv_compile::GL_VERTEX_SHADER;
const GL_FRAGMENT_SHADER: u32 = spirv_compile::GL_FRAGMENT_SHADER;
const GL_COMPUTE_SHADER: u32 = spirv_compile::GL_COMPUTE_SHADER;

// ============ TranslationResult 枚举 ============

#[test]
fn translation_result_translated_variant() {
    let result = TranslationResult::Translated("test".to_string());
    assert!(matches!(result, TranslationResult::Translated(_)));
    if let TranslationResult::Translated(src) = result {
        assert_eq!(src, "test");
    }
}

#[test]
fn translation_result_passthrough_variant() {
    let result = TranslationResult::PassThrough;
    assert!(matches!(result, TranslationResult::PassThrough));
}

#[test]
fn translation_result_failed_variant() {
    let result = TranslationResult::Failed;
    assert!(matches!(result, TranslationResult::Failed));
}

#[test]
fn translation_result_clone_and_debug() {
    let result = TranslationResult::Translated("test".to_string());
    let cloned = result.clone();
    assert!(matches!(cloned, TranslationResult::Translated(_)));
    // Debug trait 应可格式化
    let _debug_str = format!("{:?}", result);
}

// ============ 端到端翻译：vertex shader ============

#[test]
fn translate_simple_vertex_shader_succeeds() {
    let src = "#version 330 core\n\
               uniform mat4 MVP;\n\
               in vec3 Position;\n\
               void main() { gl_Position = MVP * vec4(Position, 1.0); }\n";
    let result = translate(src, GL_VERTEX_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            assert!(out.contains("#version"), "missing #version: {}", out);
            assert!(out.contains("void main"), "missing main: {}", out);
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}

#[test]
fn translate_vertex_with_ubo_succeeds() {
    let src = "#version 330\n\
               layout(std140) uniform DynamicTransforms {\n\
                   mat4 ModelViewMat;\n\
                   vec4 ColorModulator;\n\
                   vec3 ModelOffset;\n\
                   mat4 TextureMat;\n\
               };\n\
               in vec3 Position;\n\
               in vec4 Color;\n\
               out vec4 vertexColor;\n\
               void main() {\n\
                   gl_Position = ModelViewMat * vec4(Position, 1.0);\n\
                   vertexColor = Color;\n\
               }\n";
    let result = translate(src, GL_VERTEX_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            assert!(out.contains("std140"), "missing std140: {}", out);
            assert!(out.contains("ModelViewMat"), "missing UBO member: {}", out);
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}

#[test]
fn translate_vertex_330_core_profile_succeeds() {
    let src = "#version 330 core\n\
               uniform mat4 MVP;\n\
               in vec3 Position;\n\
               void main() { gl_Position = MVP * vec4(Position, 1.0); }\n";
    let result = translate(src, GL_VERTEX_SHADER);
    assert!(matches!(result, TranslationResult::Translated(_)));
}

// ============ 端到端翻译：fragment shader ============

#[test]
fn translate_simple_fragment_shader_succeeds() {
    let src = "#version 330 core\n\
               uniform sampler2D tex;\n\
               in vec2 uv;\n\
               out vec4 fragColor;\n\
               void main() { fragColor = texture(tex, uv); }\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            assert!(out.contains("texture("), "missing texture() call: {}", out);
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}

#[test]
fn translate_fragment_with_multiple_samplers_succeeds() {
    let src = "#version 450\n\
               layout(binding = 0) uniform sampler2D Sampler0;\n\
               layout(binding = 1) uniform sampler2D Sampler1;\n\
               layout(location = 0) in vec2 texCoord;\n\
               layout(location = 0) out vec4 fragColor;\n\
               void main() {\n\
                   vec4 c0 = texture(Sampler0, texCoord);\n\
                   vec4 c1 = texture(Sampler1, texCoord);\n\
                   fragColor = c0 * c1;\n\
               }\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            // 320 es 保留 binding（ES 3.1+ 合法，spike 实测），sampler 声明保留
            assert!(
                out.contains("Sampler0") && out.contains("Sampler1"),
                "sampler 应保留: {}",
                out
            );
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}

#[test]
fn translate_minecraft_realistic_fragment_succeeds() {
    let src = "#version 330\n\
               layout(std140) uniform DynamicTransforms {\n\
                   mat4 ModelViewMat;\n\
                   vec4 ColorModulator;\n\
                   vec3 ModelOffset;\n\
                   mat4 TextureMat;\n\
               };\n\
               in vec4 vertexColor;\n\
               out vec4 fragColor;\n\
               void main() {\n\
                   vec4 color = vertexColor;\n\
                   if (color.a == 0.0) {\n\
                       discard;\n\
                   }\n\
                   fragColor = color * ColorModulator;\n\
               }\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            assert!(out.contains("discard"), "missing discard: {}", out);
            assert!(
                out.contains("ColorModulator"),
                "missing UBO member: {}",
                out
            );
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}

// ============ 端到端翻译：compute shader ============

#[test]
fn translate_compute_shader_succeeds() {
    let src = "#version 450 core\n\
               layout(local_size_x = 8, local_size_y = 8) in;\n\
               layout(binding = 0, std430) buffer Buffer {\n\
                   vec4 data[];\n\
               };\n\
               void main() {\n\
                   uint idx = gl_GlobalInvocationID.x;\n\
                   data[idx] = vec4(1.0);\n\
               }\n";
    let result = translate(src, GL_COMPUTE_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            assert!(out.contains("local_size"), "missing local_size: {}", out);
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}

// ============ 翻译输出质量验证 ============

#[test]
fn translate_output_contains_gles_version() {
    let src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let result = translate(src, GL_VERTEX_SHADER);
    if let TranslationResult::Translated(out) = result {
        assert!(
            out.contains("#version 300 es")
                || out.contains("#version 310 es")
                || out.contains("#version 320 es"),
            "expected GLES version, got: {}",
            out
        );
    } else {
        panic!("expected Translated");
    }
}

#[test]
fn translate_output_contains_precision() {
    // spirv-cross 在 es_default_float/int_precision_highp=true 下自动输出 precision
    // （FS 有、VS 无且合法——VS 的 float/int 默认精度即 highp，spike 实测）。
    // 用 fragment shader 验证 FS 输出带 precision。
    let src = "#version 330 core\n\
               uniform sampler2D tex;\n\
               in vec2 uv;\n\
               out vec4 fragColor;\n\
               void main() { fragColor = texture(tex, uv); }\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    if let TranslationResult::Translated(out) = result {
        assert!(
            out.contains("precision highp float;"),
            "missing precision: {}",
            out
        );
        assert!(out.contains("precision highp int;"));
    } else {
        panic!("expected Translated");
    }
}

#[test]
fn translate_output_does_not_contain_binding_in_300_fallback() {
    // 320 主路径保留 binding（ES 3.1+ 合法）；仅在 300 es 回退时移除。
    // 此测试直接验证 300 es 的编译产物（通过 gles_compile 层）——
    // spirv_pass 层默认输出 320，binding 保留（见 translate_output_keeps_binding）。
    let src = "#version 450\n\
               layout(binding = 0) uniform sampler2D tex;\n\
               layout(location = 0) in vec2 uv;\n\
               layout(location = 0) out vec4 fragColor;\n\
               void main() { fragColor = texture(tex, uv); }\n";
    // 先编译 SPIR-V，再以 300 es 输出验证 binding 移除（300 条件 strip）
    let spv = spirv_compile::compile(src, GL_FRAGMENT_SHADER).expect("SPIR-V compile failed");
    let es300 = fluorategl::shader_translator::gles_compile::compile(&spv, 300)
        .expect("300 es output failed");
    assert!(
        !es300.contains("binding"),
        "300 es 应移除 binding: {}",
        es300
    );
}

#[test]
fn translate_output_does_not_contain_binding_in_320() {
    // 全版本剥离 binding（桌面 GL 3.3 语义：sampler/block 无 binding 声明，
    // 靠 glUniform1i/glUniformBlockBinding/glBindBufferBase API 分配；
    // spirv-cross 独立分配导致跨 stage binding mismatch，必须剥离）
    let src = "#version 450\n\
               layout(binding = 0) uniform sampler2D tex;\n\
               layout(location = 0) in vec2 uv;\n\
               layout(location = 0) out vec4 fragColor;\n\
               void main() { fragColor = texture(tex, uv); }\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    if let TranslationResult::Translated(out) = result {
        assert!(!out.contains("binding"), "320 应剥离 binding: {}", out);
    } else {
        panic!("expected Translated");
    }
}

#[test]
fn translate_output_preserves_texture_call() {
    let src = "#version 330 core\n\
               uniform sampler2D tex;\n\
               in vec2 uv;\n\
               out vec4 fragColor;\n\
               void main() { fragColor = texture(tex, uv); }\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    if let TranslationResult::Translated(out) = result {
        assert!(out.contains("texture("), "missing texture() call: {}", out);
    } else {
        panic!("expected Translated");
    }
}

// ============ 非法输入优雅失败（回退到 string_pass） ============

#[test]
fn translate_invalid_glsl_falls_back_to_string_pass() {
    let src = "#version 330 core\nthis is not valid glsl\n";
    let result = translate(src, GL_VERTEX_SHADER);
    // translate() 永不返回 Failed：无效 GLSL 回退到 string_pass
    assert!(
        matches!(
            result,
            TranslationResult::Translated(_) | TranslationResult::PassThrough
        ),
        "expected Translated/PassThrough for invalid GLSL, got {:?}",
        result
    );
}

#[test]
fn translate_syntax_error_falls_back_to_string_pass() {
    let src = "#version 330 core\nvoid main() {\n    // 缺少闭括号\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    assert!(matches!(
        result,
        TranslationResult::Translated(_) | TranslationResult::PassThrough
    ));
}

#[test]
fn translate_empty_source_falls_back_to_string_pass() {
    let result = translate("", GL_VERTEX_SHADER);
    assert!(matches!(
        result,
        TranslationResult::Translated(_) | TranslationResult::PassThrough
    ));
}

#[test]
fn translate_undefined_function_falls_back_to_string_pass() {
    let src = "#version 330 core\nvoid main() {\n    undefinedFunction();\n}\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    assert!(matches!(
        result,
        TranslationResult::Translated(_) | TranslationResult::PassThrough
    ));
}

// ============ legacy 版本拒绝 ============

#[test]
fn translate_legacy_120_upgrades_and_translates() {
    // preprocess 将 120 升级到 330 core，简单 shader 应翻译成功
    let src = "#version 120\nvoid main() {}\n";
    let result = translate(src, GL_VERTEX_SHADER);
    assert!(
        matches!(result, TranslationResult::Translated(_)),
        "expected Translated for upgraded #version 120, got {:?}",
        result
    );
}

#[test]
fn translate_legacy_150_upgrades_and_translates() {
    // preprocess 将 150 升级到 330 core，简单 shader 应翻译成功
    let src = "#version 150 core\nvoid main() {}\n";
    let result = translate(src, GL_VERTEX_SHADER);
    assert!(
        matches!(result, TranslationResult::Translated(_)),
        "expected Translated for upgraded #version 150, got {:?}",
        result
    );
}

// ============ 无效 stage ============

#[test]
fn translate_invalid_stage_falls_back_to_string_pass() {
    let src = "#version 330 core\nvoid main() {}\n";
    let result = translate(src, 0x0000);
    assert!(matches!(
        result,
        TranslationResult::Translated(_) | TranslationResult::PassThrough
    ));
}

#[test]
fn translate_undefined_stage_falls_back_to_string_pass() {
    let src = "#version 330 core\nvoid main() {}\n";
    let result = translate(src, 0x8B32); // 未定义的 stage 常量
    assert!(matches!(
        result,
        TranslationResult::Translated(_) | TranslationResult::PassThrough
    ));
}

// ============ panic 安全 ============

#[test]
fn translate_does_not_panic_on_malformed_input() {
    // 各种畸形输入都不应导致 panic
    let malformed_inputs = vec![
        "",
        "garbage",
        "#version",
        "#version 999999",
        "\0\0\0",
        "void main()",
        "#version 330\n",
    ];
    for input in malformed_inputs {
        let result = translate(input, GL_VERTEX_SHADER);
        // 只要不 panic 就行，Translated 或 Failed 都可接受
        let _ = result;
    }
}

#[test]
fn translate_does_not_panic_on_large_input() {
    let large_src = format!(
        "#version 330 core\nvoid main() {{\n{}\n}}\n",
        "float x = 0.0;\n".repeat(1000)
    );
    let result = translate(&large_src, GL_VERTEX_SHADER);
    let _ = result;
}

#[test]
fn translate_does_not_panic_with_unicode() {
    let src = "#version 330 core\n// 这是注释 with unicode\nvoid main() {}\n";
    let result = translate(src, GL_VERTEX_SHADER);
    let _ = result;
}

// ============ 一致性测试 ============

#[test]
fn translate_same_shader_produces_consistent_output() {
    let src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let result1 = translate(src, GL_VERTEX_SHADER);
    let result2 = translate(src, GL_VERTEX_SHADER);
    match (&result1, &result2) {
        (TranslationResult::Translated(out1), TranslationResult::Translated(out2)) => {
            assert_eq!(out1, out2, "output should be consistent");
        }
        _ => panic!(
            "expected both Translated, got {:?} and {:?}",
            result1, result2
        ),
    }
}

#[test]
fn translate_different_stages_produce_different_output() {
    // vertex 和 fragment shader 应产生不同的输出
    let vert_src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let frag_src =
        "#version 330 core\nout vec4 fragColor;\nvoid main() { fragColor = vec4(1.0); }\n";
    let vert_result = translate(vert_src, GL_VERTEX_SHADER);
    let frag_result = translate(frag_src, GL_FRAGMENT_SHADER);
    if let (TranslationResult::Translated(v), TranslationResult::Translated(f)) =
        (vert_result, frag_result)
    {
        assert_ne!(v, f, "vertex and fragment output should differ");
    } else {
        panic!("expected both Translated");
    }
}

// ============ clouds samplerBuffer 端到端回归 ============

#[test]
fn test_translate_clouds_sampler_buffer() {
    // clouds 特征端到端：fragment stage + isamplerBuffer + int 坐标 texelFetch。
    // 修复前 SPIR-V 管线失败会落 string_pass 兜底；T1/T2 修复后应走完整
    // SPIR-V 管线返回 Translated，且 samplerBuffer/texelFetch 原样保留。
    let src = "#version 330\n\
               uniform isamplerBuffer CloudFaces;\n\
               in vec2 vUV;\n\
               out vec4 fragColor;\n\
               void main() {\n\
                   int index = int(gl_FragCoord.x);\n\
                   vec4 color = vec4(texelFetch(CloudFaces, index));\n\
                   fragColor = color;\n\
               }\n";
    let result = translate(src, GL_FRAGMENT_SHADER);
    match result {
        TranslationResult::Translated(out) => {
            eprintln!(
                "=== test_translate_clouds_sampler_buffer translated ===\n{}",
                out
            );
            let first_line = out.lines().next().expect("output should not be empty");
            assert!(
                first_line.starts_with("#version 3") && first_line.contains("es"),
                "expected #version 3xx es first line, got: {}",
                first_line
            );
            assert!(
                out.contains("samplerBuffer"),
                "missing samplerBuffer: {}",
                out
            );
            assert!(out.contains("texelFetch"), "missing texelFetch: {}", out);
            assert!(
                out.contains("precision highp float;"),
                "missing precision: {}",
                out
            );
        }
        other => panic!("expected Translated, got {:?}", other),
    }
}
