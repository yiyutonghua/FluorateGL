//! postprocess 模块公开 API 单元测试
//!
//! 覆盖 `postprocess::post_process(src, version)` 的各种场景（OpenGL target 重构后
//! 按目标 GLES 版本条件执行）：
//! - 320 es：保留 in/out location、uniform location、binding（ES 3.2 全支持）
//! - 310 es：strip location（保守），保留 binding
//! - 300 es：strip location + 移除 binding（ES 3.0 不支持）
//! - outColorN 的 location 注入（无条件，MC framebuffer 约定）
//! - UBO 实例名移除（无条件，spirv-cross 输出 `} _20;`）
//! - atomic counter binding 修复、image format 注入（无条件）

use fluorategl::shader_translator::postprocess;

// ============ binding 移除（300 es 条件） ============

#[test]
fn postprocess_removes_binding_only_layout() {
    // layout(binding=X) 作为唯一 layout 限定符 → 整个 layout(...) 移除
    let src = "layout(binding = 0) uniform sampler2D tex;";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("binding"), "got: {}", result);
    assert!(!result.contains("layout"), "got: {}", result);
}

#[test]
fn postprocess_removes_binding_leading_in_layout() {
    // layout(binding=X, std140) → layout(std140)
    let src = "layout(binding = 0, std140) uniform Block { mat4 m; };";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("binding"));
    assert!(result.contains("layout(std140)"));
}

#[test]
fn postprocess_removes_binding_middle_in_layout() {
    // layout(std140, binding=X, column_major) → layout(std140, column_major)
    let src = "layout(std140, binding = 2, column_major) uniform Block { mat4 m; };";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("binding"));
    assert!(result.contains("std140"));
    assert!(result.contains("column_major"));
}

#[test]
fn postprocess_removes_binding_trailing_in_layout() {
    // layout(std140, binding=X) → layout(std140)
    let src = "layout(std140, binding = 1) uniform Block { mat4 m; };";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("binding"));
    assert!(result.contains("layout(std140)"));
}

#[test]
fn postprocess_removes_multiple_bindings() {
    let src =
        "layout(binding = 0) uniform sampler2D tex;\nlayout(binding = 1) uniform sampler2D tex2;";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("binding"));
}

#[test]
fn postprocess_removes_binding_case_insensitive() {
    // 大小写不敏感
    let src = "layout(BINDING = 0) uniform sampler2D tex;";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("inding"));
}

#[test]
fn postprocess_removes_binding_no_spaces() {
    // 无空格: layout(binding=0)
    let src = "layout(binding=0) uniform sampler2D tex;";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("binding"));
}

#[test]
fn postprocess_removes_binding_extra_spaces() {
    // 多空格: layout(binding  =  0)
    let src = "layout(binding  =  0) uniform sampler2D tex;";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("binding"));
}

#[test]
fn postprocess_cleans_empty_layout_parens() {
    // 移除 binding 后若 layout() 为空，整个 layout() 也移除
    let src = "layout(binding = 0) uniform sampler2D tex;";
    let result = postprocess::post_process(src, 300);
    assert!(
        !result.contains("layout()"),
        "empty layout() should be cleaned: {}",
        result
    );
}

#[test]
fn postprocess_preserves_push_constant_layout() {
    // layout(push_constant, binding=X) → layout(push_constant)
    let src = "layout(push_constant, binding = 0) uniform PushConst { mat4 m; };";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("binding"));
    assert!(result.contains("layout(push_constant)"));
}

// ============ binding 剥离（全版本，桌面语义） ============

#[test]
fn postprocess_320_strips_binding() {
    // 全版本剥离 binding（模拟桌面 GL 3.3：sampler/block 无 binding 声明，
    // 靠 glUniform1i/glUniformBlockBinding/glBindBufferBase API 分配）
    let src = "layout(binding = 0) uniform sampler2D tex;";
    let result = postprocess::post_process(src, 320);
    assert!(!result.contains("binding"), "320 应剥离 binding: {}", result);
}

#[test]
fn postprocess_310_strips_binding() {
    let src = "layout(binding = 0) uniform sampler2D tex;";
    let result = postprocess::post_process(src, 310);
    assert!(!result.contains("binding"), "310 应剥离 binding: {}", result);
}

// ============ outColorN location 注入（无条件） ============

#[test]
fn postprocess_injects_location_for_out_color() {
    let src = "out vec4 outColor0;";
    let result = postprocess::post_process(src, 320);
    assert!(
        result.contains("layout(location=0) out vec4 outColor0;"),
        "got: {}",
        result
    );
}

#[test]
fn postprocess_injects_location_for_multiple_out_colors() {
    let src = "out vec4 outColor0;\nout vec4 outColor1;\nout vec4 outColor2;";
    let result = postprocess::post_process(src, 320);
    assert!(result.contains("layout(location=0) out vec4 outColor0;"));
    assert!(result.contains("layout(location=1) out vec4 outColor1;"));
    assert!(result.contains("layout(location=2) out vec4 outColor2;"));
}

#[test]
fn postprocess_injects_location_for_out_color_with_precision() {
    // 带 precision 的 outColor
    let src = "out highp vec4 outColor0;";
    let result = postprocess::post_process(src, 320);
    assert!(result.contains("layout(location=0) out highp vec4 outColor0;"));
}

#[test]
fn postprocess_does_not_modify_non_out_color_variables() {
    // 非 outColor 命名的 out 变量不注入 location
    let src = "out vec4 fragColor;";
    let result = postprocess::post_process(src, 320);
    // fragColor 不匹配 outColorN 模式
    assert!(!result.contains("layout(location=0) out vec4 fragColor"));
}

#[test]
fn postprocess_out_color_replaces_existing_location() {
    // 320 保留 location 路径：spirv-cross 输出带 layout(location = N) 的
    // outColorN，应剥离原值再注入正确值（MC framebuffer 约定 outColorN → N）
    let src = "layout(location = 2) out vec4 outColor0;";
    let result = postprocess::post_process(src, 320);
    assert!(
        result.contains("layout(location=0) out vec4 outColor0;"),
        "outColor0 应重注为 location=0，got: {}",
        result
    );
}

// ============ 版本条件 strip：300/310 strip、320 保留 ============

#[test]
fn postprocess_300_strips_varying_locations() {
    let src = "#version 300 es\nlayout(location = 0) in vec2 texCoord0;\nlayout(location = 0) out vec4 fragColor;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("layout(location"));
}

#[test]
fn postprocess_300_strips_uniform_location() {
    // ES 3.0 不支持 uniform location（spike_g 实测 300 es 输出带 location 编译失败）
    let src = "#version 300 es\nlayout(location = 0) uniform mat4 MVP;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 300);
    assert!(
        result.contains("uniform mat4 MVP;"),
        "uniform 应保留但无 location: {}",
        result
    );
}

#[test]
fn postprocess_310_strips_locations() {
    // 310 保守 strip（跨 stage 计数一致性 + 老驱动兼容）
    let src = "#version 310 es\nlayout(location = 0) uniform mat4 MVP;\nlayout(location = 0) in vec3 Position;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 310);
    assert!(
        !result.contains("location"),
        "310 不应残留 location: {}",
        result
    );
}

#[test]
fn postprocess_320_strips_locations() {
    // 全版本剥离 location（桌面 GL 3.3：varying 按名匹配、uniform 按名查询）
    let src = "#version 320 es\nlayout(location = 0) uniform mat4 MVP;\nlayout(location = 0) in vec3 Position;\nlayout(location = 0) out vec4 fragColor;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 320);
    assert!(
        !result.contains("location"),
        "320 不应残留 location: {}",
        result
    );
    assert!(result.contains("uniform mat4 MVP;"));
    assert!(result.contains("in vec3 Position;"));
    assert!(result.contains("out vec4 fragColor;"));
}

// ============ 组合场景 ============

#[test]
fn postprocess_full_gles_output_cleanup() {
    // 模拟 spirv-cross 320 es 输出：binding/location 全剥离、outColor 重注
    let src = "#version 320 es\nlayout(binding = 0) uniform sampler2D Sampler0;\nlayout(location = 0) in vec2 texCoord;\nout vec4 outColor0;\nvoid main() {\n    outColor0 = texture(Sampler0, texCoord);\n}\n";
    let result = postprocess::post_process(src, 320);
    // 全版本剥离 binding 与 in/out location（桌面语义）
    assert!(!result.contains("binding"), "320 应剥离 binding: {}", result);
    assert!(
        !result.contains("layout(location = 0) in"),
        "320 应剥离 in location: {}",
        result
    );
    // outColor0 有 location（重注，Sodium 依赖 outColorN → attachment N）
    assert!(result.contains("layout(location=0) out vec4 outColor0;"));
    // shader body 保留
    assert!(result.contains("texture(Sampler0, texCoord)"));
}

#[test]
fn postprocess_preserves_shader_body() {
    let src = "#version 320 es\nvoid main() {\n    gl_Position = vec4(1.0);\n}\n";
    let result = postprocess::post_process(src, 320);
    assert!(result.contains("void main() {"));
    assert!(result.contains("gl_Position = vec4(1.0);"));
    assert!(result.contains("}"));
}

#[test]
fn postprocess_handles_empty_input() {
    // 空输入原样返回（无 precision 注入——spirv-cross 负责输出 precision）
    let result = postprocess::post_process("", 320);
    assert_eq!(result, "");
}

#[test]
fn postprocess_handles_input_without_version() {
    // 无 #version 的输入也应正常处理（原样返回）
    let src = "void main() {}\n";
    let result = postprocess::post_process(src, 320);
    assert!(result.contains("void main() {}"));
}

#[test]
fn postprocess_strips_layout_location_on_in_300() {
    // 300 回退：layout(location=X) in 的 location 应被移除（让 GLES linker 按名匹配）
    let src = "#version 300 es\nlayout(location = 0) in vec2 texCoord;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 300);
    // location 被移除，变量声明保留
    assert!(!result.contains("layout(location = 0) in"));
    assert!(result.contains("in vec2 texCoord;"));
}

#[test]
fn postprocess_preserves_layout_std140_on_ubo() {
    // UBO 的 std140 不应被移除（300 移除 binding 时）
    let src =
        "#version 300 es\nlayout(std140, binding = 0) uniform Block { mat4 m; };\nvoid main() {}\n";
    let result = postprocess::post_process(src, 300);
    assert!(result.contains("layout(std140)"));
    assert!(!result.contains("binding"));
}

// ============ atomic counter binding 修复 ============

#[test]
fn postprocess_fixes_atomic_counter_offset_to_binding() {
    // spirv-cross 输出 layout(offset = N)，GLES 要求 layout(binding = N)
    let src = "#version 320 es\nlayout(offset = 0) uniform atomic_uint counter;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 320);
    assert!(
        result.contains("layout(binding = 0) uniform atomic_uint"),
        "expected binding, got: {}",
        result
    );
    assert!(!result.contains("offset"));
}

#[test]
fn postprocess_fixes_atomic_counter_offset_with_other_qualifier() {
    // layout(offset = N, X) → layout(binding = N, X)
    let src = "#version 320 es\nlayout(offset = 2, std140) uniform atomic_uint counter;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 320);
    assert!(result.contains("binding = 2"));
    assert!(!result.contains("offset"));
}

// ============ image format 注入 ============

#[test]
fn postprocess_injects_format_for_writeonly_image() {
    let src = "#version 320 es\nuniform writeonly highp image2D dest1;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 320);
    assert!(
        result.contains("layout(binding = 0, r32f) uniform writeonly highp image2D dest1;"),
        "expected layout with uniform and r32f, got: {}",
        result
    );
}

#[test]
fn postprocess_injects_format_for_readable_image() {
    // 非 writeonly image 默认 r32ui
    let src = "#version 320 es\nuniform highp image2D dest1;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 320);
    assert!(
        result.contains("layout(binding = 0, r32ui) uniform highp image2D dest1;"),
        "expected layout with uniform and r32ui, got: {}",
        result
    );
}

#[test]
fn postprocess_injects_incremental_binding_for_multiple_images() {
    let src = "#version 320 es\nuniform writeonly highp image2D a;\nuniform writeonly highp image2D b;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 320);
    assert!(result.contains("binding = 0, r32f) uniform writeonly highp image2D a;"));
    assert!(result.contains("binding = 1, r32f) uniform writeonly highp image2D b;"));
}

#[test]
fn postprocess_skips_image_with_existing_layout() {
    // 已有 layout( 的 image 不应被重复注入
    let src =
        "#version 320 es\nlayout(r32f) uniform writeonly highp image2D dest1;\nvoid main() {}\n";
    let result = postprocess::post_process(src, 320);
    // 原有 layout(r32f) 保留，不额外注入
    assert!(result.contains("layout(r32f)"));
    assert!(!result.contains("binding = 0, r32f) writeonly"));
}

// ============ UBO 实例名移除（无条件） ============

#[test]
fn postprocess_strips_ubo_instance_name() {
    // 端到端：spirv-cross 输出（带实例名） → post_process → 实例名移除
    // 原生 UBO 块保留（不再拆解为 standalone uniform）
    let src = "#version 320 es\nlayout(std140) uniform DynamicTransforms\n{\n    mat4 ModelViewMat;\n    mat4 ProjMat;\n} _20;\nin vec3 Position;\nvoid main()\n{\n    gl_Position = (_20.ProjMat * _20.ModelViewMat) * vec4(Position, 1.0);\n}\n";
    let result = postprocess::post_process(src, 320);
    assert!(
        !result.contains("} _20;"),
        "instance name should be removed, got: {}",
        result
    );
    assert!(
        !result.contains("_20."),
        "instance reference should be replaced, got: {}",
        result
    );
    // 原生 UBO 块应保留（不再有 unwrap_generated_ubo）
    assert!(
        result.contains("uniform DynamicTransforms"),
        "原生 UBO 应保留，got: {}",
        result
    );
    // 成员名应保留（用于 glGetUniformLocation 查询）
    assert!(result.contains("ModelViewMat"));
    assert!(result.contains("ProjMat"));
}

#[test]
fn postprocess_ubo_instance_name_300() {
    // 300 回退同样移除实例名（binding/location strip 不影响）
    let src = "#version 300 es\nlayout(std140) uniform Block\n{\n    mat4 m;\n} _10;\nvoid main()\n{\n    gl_Position = _10.m * vec4(0.0);\n}\n";
    let result = postprocess::post_process(src, 300);
    assert!(!result.contains("} _10;"), "got: {}", result);
    assert!(!result.contains("_10."), "got: {}", result);
    assert!(result.contains("uniform Block"), "got: {}", result);
}
