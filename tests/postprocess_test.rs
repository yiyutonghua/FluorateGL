//! postprocess 模块公开 API 单元测试
//!
//! 覆盖 `postprocess::post_process` 的各种场景：
//! - layout(binding=X) 移除的各种形式（唯一项、前导、中间、尾部）
//! - 空 layout 括号清理
//! - outColorN 的 location 注入
//! - precision highp float/int 声明注入与替换
//! - 组合场景

use fluorategl::shader_translator::postprocess;

// ============ binding 移除 ============

#[test]
fn postprocess_removes_binding_only_layout() {
    // layout(binding=X) 作为唯一 layout 限定符 → 整个 layout(...) 移除
    let src = "layout(binding = 0) uniform sampler2D tex;";
    let result = postprocess::post_process(src);
    assert!(!result.contains("binding"), "got: {}", result);
    assert!(!result.contains("layout"), "got: {}", result);
}

#[test]
fn postprocess_removes_binding_leading_in_layout() {
    // layout(binding=X, std140) → layout(std140)
    let src = "layout(binding = 0, std140) uniform Block { mat4 m; };";
    let result = postprocess::post_process(src);
    assert!(!result.contains("binding"));
    assert!(result.contains("layout(std140)"));
}

#[test]
fn postprocess_removes_binding_middle_in_layout() {
    // layout(std140, binding=X, column_major) → layout(std140, column_major)
    let src = "layout(std140, binding = 2, column_major) uniform Block { mat4 m; };";
    let result = postprocess::post_process(src);
    assert!(!result.contains("binding"));
    assert!(result.contains("std140"));
    assert!(result.contains("column_major"));
}

#[test]
fn postprocess_removes_binding_trailing_in_layout() {
    // layout(std140, binding=X) → layout(std140)
    let src = "layout(std140, binding = 1) uniform Block { mat4 m; };";
    let result = postprocess::post_process(src);
    assert!(!result.contains("binding"));
    assert!(result.contains("layout(std140)"));
}

#[test]
fn postprocess_removes_multiple_bindings() {
    let src = "layout(binding = 0) uniform sampler2D tex;\nlayout(binding = 1) uniform sampler2D tex2;";
    let result = postprocess::post_process(src);
    assert!(!result.contains("binding"));
}

#[test]
fn postprocess_removes_binding_case_insensitive() {
    // 大小写不敏感
    let src = "layout(BINDING = 0) uniform sampler2D tex;";
    let result = postprocess::post_process(src);
    assert!(!result.contains("inding"));
}

#[test]
fn postprocess_removes_binding_no_spaces() {
    // 无空格: layout(binding=0)
    let src = "layout(binding=0) uniform sampler2D tex;";
    let result = postprocess::post_process(src);
    assert!(!result.contains("binding"));
}

#[test]
fn postprocess_removes_binding_extra_spaces() {
    // 多空格: layout(binding  =  0)
    let src = "layout(binding  =  0) uniform sampler2D tex;";
    let result = postprocess::post_process(src);
    assert!(!result.contains("binding"));
}

#[test]
fn postprocess_cleans_empty_layout_parens() {
    // 移除 binding 后若 layout() 为空，整个 layout() 也移除
    let src = "layout(binding = 0) uniform sampler2D tex;";
    let result = postprocess::post_process(src);
    assert!(!result.contains("layout()"), "empty layout() should be cleaned: {}", result);
}

#[test]
fn postprocess_preserves_push_constant_layout() {
    // layout(push_constant, binding=X) → layout(push_constant)
    let src = "layout(push_constant, binding = 0) uniform PushConst { mat4 m; };";
    let result = postprocess::post_process(src);
    assert!(!result.contains("binding"));
    assert!(result.contains("layout(push_constant)"));
}

// ============ outColorN location 注入 ============

#[test]
fn postprocess_injects_location_for_out_color() {
    let src = "out vec4 outColor0;";
    let result = postprocess::post_process(src);
    assert!(result.contains("layout(location=0) out vec4 outColor0;"), "got: {}", result);
}

#[test]
fn postprocess_injects_location_for_multiple_out_colors() {
    let src = "out vec4 outColor0;\nout vec4 outColor1;\nout vec4 outColor2;";
    let result = postprocess::post_process(src);
    assert!(result.contains("layout(location=0) out vec4 outColor0;"));
    assert!(result.contains("layout(location=1) out vec4 outColor1;"));
    assert!(result.contains("layout(location=2) out vec4 outColor2;"));
}

#[test]
fn postprocess_injects_location_for_out_color_with_precision() {
    // 带 precision 的 outColor
    let src = "out highp vec4 outColor0;";
    let result = postprocess::post_process(src);
    assert!(result.contains("layout(location=0) out highp vec4 outColor0;"));
}

#[test]
fn postprocess_does_not_modify_non_out_color_variables() {
    // 非 outColor 命名的 out 变量不注入 location
    let src = "out vec4 fragColor;";
    let result = postprocess::post_process(src);
    // fragColor 不匹配 outColorN 模式
    assert!(!result.contains("layout(location=0) out vec4 fragColor"));
}

// ============ precision 声明 ============

#[test]
fn postprocess_injects_precision_when_missing() {
    let src = "#version 320 es\nvoid main() {}\n";
    let result = postprocess::post_process(src);
    assert!(result.contains("precision highp float;"), "got: {}", result);
    assert!(result.contains("precision highp int;"));
}

#[test]
fn postprocess_replaces_existing_precision() {
    // 已有 mediump float → 替换为 highp float
    let src = "#version 320 es\nprecision mediump float;\nvoid main() {}\n";
    let result = postprocess::post_process(src);
    assert!(result.contains("precision highp float;"));
    assert!(!result.contains("precision mediump float;"));
}

#[test]
fn postprocess_replaces_multiple_precisions() {
    // 已有 mediump float + lowp int → 全部替换为 highp
    let src = "#version 320 es\nprecision mediump float;\nprecision lowp int;\nvoid main() {}\n";
    let result = postprocess::post_process(src);
    assert!(result.contains("precision highp float;"));
    assert!(result.contains("precision highp int;"));
    assert!(!result.contains("precision mediump"));
    assert!(!result.contains("precision lowp"));
}

#[test]
fn postprocess_inserts_precision_after_version() {
    let src = "#version 320 es\nvoid main() {}\n";
    let result = postprocess::post_process(src);
    let version_pos = result.find("#version").unwrap();
    let precision_pos = result.find("precision highp float;").unwrap();
    assert!(precision_pos > version_pos, "precision should come after #version");
}

// ============ 组合场景 ============

#[test]
fn postprocess_full_gles_output_cleanup() {
    // 模拟 spirv-cross 输出，包含 binding、outColor、无 precision
    let src = "#version 320 es\nlayout(binding = 0) uniform sampler2D Sampler0;\nlayout(location = 0) in vec2 texCoord;\nout vec4 outColor0;\nvoid main() {\n    outColor0 = texture(Sampler0, texCoord);\n}\n";
    let result = postprocess::post_process(src);
    // binding 被移除
    assert!(!result.contains("binding"));
    // outColor0 有 location
    assert!(result.contains("layout(location=0) out vec4 outColor0;"));
    // precision 被注入
    assert!(result.contains("precision highp float;"));
    assert!(result.contains("precision highp int;"));
    // shader body 保留
    assert!(result.contains("texture(Sampler0, texCoord)"));
}

#[test]
fn postprocess_preserves_shader_body() {
    let src = "#version 320 es\nvoid main() {\n    gl_Position = vec4(1.0);\n}\n";
    let result = postprocess::post_process(src);
    assert!(result.contains("void main() {"));
    assert!(result.contains("gl_Position = vec4(1.0);"));
    assert!(result.contains("}"));
}

#[test]
fn postprocess_handles_empty_input() {
    let result = postprocess::post_process("");
    // 空输入应至少注入 precision
    assert!(result.contains("precision highp float;"));
}

#[test]
fn postprocess_handles_input_without_version() {
    // 无 #version 的输入也应正常处理
    let src = "void main() {}\n";
    let result = postprocess::post_process(src);
    assert!(result.contains("precision highp float;"));
    assert!(result.contains("void main() {}"));
}

#[test]
fn postprocess_preserves_layout_location_on_in() {
    // layout(location=X) in 不应被移除（只移除 binding）
    let src = "#version 320 es\nlayout(location = 0) in vec2 texCoord;\nvoid main() {}\n";
    let result = postprocess::post_process(src);
    assert!(result.contains("layout(location = 0) in vec2 texCoord;"));
}

#[test]
fn postprocess_preserves_layout_std140_on_ubo() {
    // UBO 的 std140 不应被移除
    let src = "#version 320 es\nlayout(std140, binding = 0) uniform Block { mat4 m; };\nvoid main() {}\n";
    let result = postprocess::post_process(src);
    assert!(result.contains("layout(std140)"));
    assert!(!result.contains("binding"));
}
