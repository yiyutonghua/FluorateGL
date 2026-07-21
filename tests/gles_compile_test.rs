//! gles_compile 模块公开 API 单元测试
//!
//! 覆盖 `gles_compile::compile` 和 `gles_compile::gles_version_candidates`：
//! - GLES 版本推导（桌面 330/450 → ES 320/310/300，低版本 → ES 310/300）
//! - SPIR-V → GLSL ES 转换成功（vertex/fragment/compute）
//! - 无效 SPIR-V 返回 Err
//! - 转换后输出包含 GLSL ES 版本指令
//! - 转换后输出经后处理（binding 移除、precision 注入）
//!
//! 注意：测试需要先通过 spirv_compile 生成 SPIR-V，再调用 gles_compile 转换。

use fluorategl::shader_translator::{gles_compile, spirv_compile};

// ============ gles_version_candidates ============

#[test]
fn gles_version_candidates_for_330() {
    let src = "#version 330 core\nvoid main() {}\n";
    let candidates = gles_compile::gles_version_candidates(src);
    assert_eq!(candidates, vec![320, 310, 300]);
}

#[test]
fn gles_version_candidates_for_450() {
    let src = "#version 450 core\nvoid main() {}\n";
    let candidates = gles_compile::gles_version_candidates(src);
    assert_eq!(candidates, vec![320, 310, 300]);
}

#[test]
fn gles_version_candidates_for_460() {
    let src = "#version 460 core\nvoid main() {}\n";
    let candidates = gles_compile::gles_version_candidates(src);
    assert_eq!(candidates, vec![320, 310, 300]);
}

#[test]
fn gles_version_candidates_for_150() {
    // #version 150 < 330，走默认分支
    let src = "#version 150 core\nvoid main() {}\n";
    let candidates = gles_compile::gles_version_candidates(src);
    assert_eq!(candidates, vec![310, 300]);
}

#[test]
fn gles_version_candidates_for_120() {
    let src = "#version 120\nvoid main() {}\n";
    let candidates = gles_compile::gles_version_candidates(src);
    assert_eq!(candidates, vec![310, 300]);
}

#[test]
fn gles_version_candidates_for_no_version() {
    // 无 #version 指令，默认 150
    let src = "void main() {}\n";
    let candidates = gles_compile::gles_version_candidates(src);
    assert_eq!(candidates, vec![310, 300]);
}

#[test]
fn gles_version_candidates_for_empty_input() {
    let candidates = gles_compile::gles_version_candidates("");
    assert_eq!(candidates, vec![310, 300]);
}

#[test]
fn gles_version_candidates_returns_non_empty() {
    let src = "#version 330\nvoid main() {}\n";
    let candidates = gles_compile::gles_version_candidates(src);
    assert!(!candidates.is_empty());
}

#[test]
fn gles_version_candidates_sorted_descending() {
    // 候选版本应按降序排列（先尝试高版本）
    let src = "#version 330\nvoid main() {}\n";
    let candidates = gles_compile::gles_version_candidates(src);
    for i in 1..candidates.len() {
        assert!(
            candidates[i - 1] > candidates[i],
            "candidates not descending: {:?}",
            candidates
        );
    }
}

// ============ compile: SPIR-V → GLSL ES 转换 ============

/// 辅助函数：将 GLSL 编译为 SPIR-V
fn make_spirv(src: &str, stage: u32) -> Vec<u32> {
    spirv_compile::compile(src, stage).expect("failed to compile GLSL to SPIR-V")
}

#[test]
fn compile_vertex_spv_to_gles_succeeds() {
    let src = "#version 330 core\n\
               uniform mat4 MVP;\n\
               in vec3 Position;\n\
               void main() { gl_Position = MVP * vec4(Position, 1.0); }\n";
    let spv = make_spirv(src, spirv_compile::GL_VERTEX_SHADER);
    let result = gles_compile::compile(&spv, 300);
    assert!(result.is_ok(), "expected Ok, got Err: {:?}", result.err());
    let gles_src = result.unwrap();
    assert!(
        gles_src.contains("#version"),
        "missing #version: {}",
        gles_src
    );
}

#[test]
fn compile_fragment_spv_to_gles_succeeds() {
    let src = "#version 330 core\n\
               uniform sampler2D tex;\n\
               in vec2 uv;\n\
               out vec4 fragColor;\n\
               void main() { fragColor = texture(tex, uv); }\n";
    let spv = make_spirv(src, spirv_compile::GL_FRAGMENT_SHADER);
    let result = gles_compile::compile(&spv, 300);
    assert!(result.is_ok(), "got Err: {:?}", result.err());
    let gles_src = result.unwrap();
    assert!(
        gles_src.contains("texture("),
        "missing texture() call: {}",
        gles_src
    );
}

#[test]
fn compile_compute_spv_to_gles_succeeds() {
    let src = "#version 450 core\n\
               layout(local_size_x = 8) in;\n\
               layout(binding = 0, std430) buffer Buffer {\n\
                   vec4 data[];\n\
               };\n\
               void main() {\n\
                   uint idx = gl_GlobalInvocationID.x;\n\
                   data[idx] = vec4(1.0);\n\
               }\n";
    let spv = make_spirv(src, spirv_compile::GL_COMPUTE_SHADER);
    let result = gles_compile::compile(&spv, 310);
    // compute shader 需要 GLES 310+
    assert!(result.is_ok(), "got Err: {:?}", result.err());
}

#[test]
fn compile_spv_to_gles_320() {
    let src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let spv = make_spirv(src, spirv_compile::GL_VERTEX_SHADER);
    let result = gles_compile::compile(&spv, 320);
    assert!(result.is_ok());
    let gles_src = result.unwrap();
    assert!(
        gles_src.contains("#version 320 es"),
        "expected #version 320 es: {}",
        gles_src
    );
}

#[test]
fn compile_spv_to_gles_310() {
    let src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let spv = make_spirv(src, spirv_compile::GL_VERTEX_SHADER);
    let result = gles_compile::compile(&spv, 310);
    assert!(result.is_ok());
    let gles_src = result.unwrap();
    assert!(
        gles_src.contains("#version 310 es"),
        "expected #version 310 es: {}",
        gles_src
    );
}

#[test]
fn compile_spv_to_gles_300() {
    let src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let spv = make_spirv(src, spirv_compile::GL_VERTEX_SHADER);
    let result = gles_compile::compile(&spv, 300);
    assert!(result.is_ok());
    let gles_src = result.unwrap();
    assert!(
        gles_src.contains("#version 300 es"),
        "expected #version 300 es: {}",
        gles_src
    );
}

// ============ compile: 后处理验证 ============

#[test]
fn compile_removes_binding_in_output() {
    // 输入的 GLSL 有 binding，转换后应被后处理移除
    let src = "#version 450\n\
               layout(binding = 0) uniform sampler2D Sampler0;\n\
               layout(location = 0) in vec2 texCoord;\n\
               layout(location = 0) out vec4 fragColor;\n\
               void main() { fragColor = texture(Sampler0, texCoord); }\n";
    let spv = make_spirv(src, spirv_compile::GL_FRAGMENT_SHADER);
    let result = gles_compile::compile(&spv, 300);
    assert!(result.is_ok());
    let gles_src = result.unwrap();
    assert!(
        !gles_src.contains("binding"),
        "binding should be removed: {}",
        gles_src
    );
}

#[test]
fn compile_injects_precision_in_output() {
    let src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let spv = make_spirv(src, spirv_compile::GL_VERTEX_SHADER);
    let result = gles_compile::compile(&spv, 300);
    assert!(result.is_ok());
    let gles_src = result.unwrap();
    assert!(
        gles_src.contains("precision highp float;"),
        "missing precision: {}",
        gles_src
    );
    assert!(gles_src.contains("precision highp int;"));
}

#[test]
fn compile_preserves_shader_logic() {
    let src = "#version 330 core\n\
               uniform mat4 MVP;\n\
               in vec3 Position;\n\
               void main() { gl_Position = MVP * vec4(Position, 1.0); }\n";
    let spv = make_spirv(src, spirv_compile::GL_VERTEX_SHADER);
    let result = gles_compile::compile(&spv, 300);
    assert!(result.is_ok());
    let gles_src = result.unwrap();
    // 矩阵乘法和 vec4 构造应保留
    assert!(gles_src.contains("vec4"));
    assert!(gles_src.contains("void main"));
}

#[test]
fn compile_ubo_output_has_std140_layout() {
    let src = "#version 330\n\
               layout(std140) uniform DynamicTransforms {\n\
                   mat4 ModelViewMat;\n\
               };\n\
               in vec3 Position;\n\
               void main() { gl_Position = ModelViewMat * vec4(Position, 1.0); }\n";
    let spv = make_spirv(src, spirv_compile::GL_VERTEX_SHADER);
    let result = gles_compile::compile(&spv, 300);
    assert!(result.is_ok());
    let gles_src = result.unwrap();
    // UBO 的 std140 layout 应保留（后处理只移除 binding）
    assert!(gles_src.contains("std140"), "missing std140: {}", gles_src);
}

// ============ compile: 无效 SPIR-V ============

#[test]
fn compile_empty_spv_returns_err() {
    let result = gles_compile::compile(&[], 300);
    assert!(result.is_err(), "expected Err for empty SPIR-V");
}

#[test]
fn compile_invalid_spv_returns_err() {
    // 非 SPIR-V 魔数
    let fake_spv: Vec<u32> = vec![0xDEADBEEF, 0, 0, 0, 0];
    let result = gles_compile::compile(&fake_spv, 300);
    assert!(result.is_err(), "expected Err for invalid SPIR-V");
}

#[test]
fn compile_truncated_spv_returns_err() {
    // 只有魔数，缺少其他头部字段
    let truncated = vec![0x07230203u32];
    let result = gles_compile::compile(&truncated, 300);
    assert!(result.is_err(), "expected Err for truncated SPIR-V");
}

// ============ compile: 多版本尝试 ============

#[test]
fn compile_fallback_through_gles_versions() {
    // 使用候选列表中的所有版本都能成功编译简单 shader
    let src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let spv = make_spirv(src, spirv_compile::GL_VERTEX_SHADER);
    for version in [320, 310, 300] {
        let result = gles_compile::compile(&spv, version);
        assert!(
            result.is_ok(),
            "GLES {} failed: {:?}",
            version,
            result.err()
        );
    }
}
