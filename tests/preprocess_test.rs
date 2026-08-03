//! preprocess 模块公开 API 单元测试
//!
//! 覆盖 `preprocess::preprocess` 和 `preprocess::extract_version` 的各种场景：
//! - 版本指令处理（无版本、低版本、330-440 升级、>= 450 保持）
//! - #line 指令移除
//! - in/out 变量 location 注入
//! - non-opaque uniform 包装进 UBO（Vulkan target 拒绝独立 non-opaque uniform，
//!   不注入 location；7a39023/bae79dc 起）
//! - UBO/SSBO binding 注入
//! - 已有 layout 限定符的跳过逻辑
//! - 组合场景与边界输入

use fluorategl::shader_translator::preprocess;

// ============ extract_version ============

#[test]
fn extract_version_returns_first_version_line() {
    let src = "#version 330 core\nvoid main() {}\n";
    assert_eq!(preprocess::extract_version(src), Some("#version 330 core"));
}

#[test]
fn extract_version_returns_none_when_missing() {
    let src = "void main() {}\n";
    assert_eq!(preprocess::extract_version(src), None);
}

#[test]
fn extract_version_skips_leading_whitespace() {
    let src = "   #version 450\nvoid main() {}\n";
    assert_eq!(preprocess::extract_version(src), Some("   #version 450"));
}

#[test]
fn extract_version_returns_none_for_empty_input() {
    assert_eq!(preprocess::extract_version(""), None);
}

#[test]
fn extract_version_returns_first_when_multiple() {
    let src = "#version 330\n#version 450\nvoid main() {}\n";
    assert_eq!(preprocess::extract_version(src), Some("#version 330"));
}

// ============ preprocess: 版本指令处理 ============

#[test]
fn preprocess_inserts_450_when_no_version() {
    let src = "void main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 450 core"),
        "expected #version 450 core, got: {}",
        result
    );
}

#[test]
fn preprocess_upgrades_330_to_450() {
    let src = "#version 330\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 450 core"),
        "expected upgrade to 450, got: {}",
        result
    );
}

#[test]
fn preprocess_upgrades_440_to_450() {
    let src = "#version 440 core\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 450 core"),
        "expected upgrade to 450, got: {}",
        result
    );
}

#[test]
fn preprocess_keeps_450_unchanged() {
    let src = "#version 450 core\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 450 core"),
        "expected 450 unchanged, got: {}",
        result
    );
}

#[test]
fn preprocess_keeps_460_unchanged() {
    let src = "#version 460 core\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 460 core"),
        "expected 460 unchanged, got: {}",
        result
    );
}

#[test]
fn preprocess_upgrades_low_desktop_version_to_450() {
    // 桌面 GLSL < 460 统一升级到 450 core（Vulkan target 需 core profile +
    // layout(binding) 需 420+，7a39023 起统一策略）
    let src = "#version 120\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 450 core"),
        "expected upgrade to 450 core, got: {}",
        result
    );
}

#[test]
fn preprocess_upgrades_150_to_450() {
    let src = "#version 150 core\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 450 core"),
        "expected upgrade to 450 core, got: {}",
        result
    );
}

#[test]
fn preprocess_keeps_es_version_unchanged() {
    // GLSL ES 版本（含 es 后缀）保持不变，语法与桌面 GLSL 不兼容
    let src = "#version 300 es\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 300 es"),
        "expected 300 es unchanged, got: {}",
        result
    );
}

#[test]
fn preprocess_es_detection_not_misled_by_es_substring_in_comment() {
    // 之前用 contains("es") 检测 ES 版本，会误匹配 meshes/textures/harness/entities
    // 等含 "es" 子串的注释，导致桌面版本被误判为 ES 而跳过升级。
    // 这里用含 "meshes" 注释的桌面 330 shader 验证：应升级到 450 core，而非保持 330。
    let src = "#version 330 // entity meshes shader\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 450 core"),
        "expected upgrade to 450 core (not misjudged as ES), got: {}",
        result
    );
}

#[test]
fn preprocess_binding_counter_advances_past_existing_binding() {
    // 已有 binding 的 UBO 应推进 counter，避免后续注入的 binding 与已有值冲突。
    // layout(std140, binding=2) uniform A; 后跟 layout(std140) uniform B;
    // B 应被注入 binding=3（而非 0，避免与 A 的 binding=2 冲突区）。
    let src = "#version 450 core\n\
        layout(std140, binding = 2) uniform A { mat4 a; };\n\
        layout(std140) uniform B { mat4 b; };\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.contains("layout(std140, binding = 2) uniform A"),
        "existing binding should be preserved, got: {}",
        result
    );
    assert!(
        result.contains("layout(std140, binding=3) uniform B"),
        "B should get binding=3 (advance past existing 2), got: {}",
        result
    );
}

#[test]
fn preprocess_injects_location_for_in_with_trailing_comment() {
    // 行尾带注释的 in/out 声明也应被注入 location
    let src = "#version 450 core\nin vec4 color; // vertex color\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.contains("layout(location=0) in vec4 color;"),
        "expected location injected despite trailing comment, got: {}",
        result
    );
}

#[test]
fn preprocess_keeps_310_es_unchanged() {
    let src = "#version 310 es\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.starts_with("#version 310 es"),
        "expected 310 es unchanged, got: {}",
        result
    );
}

#[test]
fn preprocess_strips_core_profile_on_upgrade() {
    // #version 330 core 升级到 #version 450 core（保留 core profile）
    let src = "#version 330 core\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("#version 450 core"));
}

// ============ preprocess: #line 指令移除 ============

#[test]
fn preprocess_removes_line_directives() {
    let src = "#version 330\n#line 0 2\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        !result.contains("#line"),
        "result still contains #line: {}",
        result
    );
}

#[test]
fn preprocess_removes_multiple_line_directives() {
    let src = "#version 330\n#line 0 2\n#line 5 3\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(!result.contains("#line"));
}

#[test]
fn preprocess_removes_indented_line_directives() {
    let src = "#version 330\n  #line 0 2\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(!result.contains("#line"));
}

// ============ preprocess: in/out location 注入 ============

#[test]
fn preprocess_injects_location_for_in_variables() {
    let src = "#version 330\nin vec4 color;\nin vec2 uv;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.contains("layout(location=0) in vec4 color;"),
        "got: {}",
        result
    );
    assert!(
        result.contains("layout(location=1) in vec2 uv;"),
        "got: {}",
        result
    );
}

#[test]
fn preprocess_injects_location_for_out_variables() {
    let src = "#version 330\nout vec4 fragColor;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.contains("layout(location=0) out vec4 fragColor;"),
        "got: {}",
        result
    );
}

#[test]
fn preprocess_injects_incrementing_locations_for_in_and_out() {
    // in 和 out 使用独立的 location 计数器，分别从 0 开始
    // （不同接口空间，保证 VS out 和 FS in 跨 stage 一致）
    let src = "#version 330\nin vec4 color;\nout vec4 fragColor;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("layout(location=0) in vec4 color;"));
    assert!(result.contains("layout(location=0) out vec4 fragColor;"));
}

#[test]
fn preprocess_skips_existing_location_on_in_out() {
    let src =
        "#version 330\nlayout(location=5) in vec4 color;\nout vec4 fragColor;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("layout(location=5) in vec4 color;"));
    // 缺少 location 的 out 仍从 0 开始
    assert!(result.contains("layout(location=0) out vec4 fragColor;"));
}

// ============ preprocess: uniform 处理（UBO 包装） ============

#[test]
fn preprocess_packs_non_opaque_uniforms_into_ubo() {
    // Vulkan target 拒绝独立 non-opaque uniform（必须包装进 UBO），
    // 不注入 location（7a39023/bae79dc）。块名按 stage 命名（VS→UniformBlockVS），
    // binding 从 0 开始（无已有 binding 时）。
    let src = "#version 330\nuniform mat4 MVP;\nuniform vec3 color;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.contains("layout(std140, binding = 0) uniform UniformBlockVS"),
        "expected UBO wrapper, got: {}",
        result
    );
    assert!(result.contains("mat4 MVP;"), "MVP member should be in UBO, got: {}", result);
    assert!(result.contains("vec3 color;"), "color member should be in UBO, got: {}", result);
    assert!(
        !result.contains("layout(location="),
        "location should not be injected, got: {}",
        result
    );
}

#[test]
fn preprocess_skips_sampler_uniform_location_injection() {
    // sampler 是 opaque，不应注入 location（binding 由 shaderc auto_bind_uniforms 分配）；
    // non-opaque MVP 被包装进 UBO 而非注入 location
    let src = "#version 330\nuniform sampler2D tex;\nuniform mat4 MVP;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        !result.contains("layout(location=0) uniform sampler2D"),
        "sampler should not get location, got: {}",
        result
    );
    assert!(result.contains("mat4 MVP;"), "MVP should be packed into UBO, got: {}", result);
    assert!(
        !result.contains("layout(location="),
        "location should not be injected, got: {}",
        result
    );
}

#[test]
fn preprocess_skips_texture_and_image_uniforms() {
    // texture2D/image2D 无 location（负断言，旧语义保留）；
    // scale 被包装进 UBO 而非注入 location
    let src = "#version 330\nuniform texture2D tex;\nuniform image2D img;\nuniform float scale;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(!result.contains("layout(location=0) uniform texture2D"));
    assert!(!result.contains("layout(location=0) uniform image2D"));
    assert!(result.contains("float scale;"), "scale should be packed into UBO, got: {}", result);
    assert!(
        !result.contains("layout(location="),
        "location should not be injected, got: {}",
        result
    );
}

#[test]
fn preprocess_skips_uniform_block_location_injection() {
    // uniform block 不应注入 location（负断言保留）；
    // scale 被包装进 UniformBlockVS（binding=0），MyBlock 由 inject_missing_bindings
    // 分配 binding=1（UniformBlockVS 先占 0），均不出现 location
    let src = "#version 330\nuniform MyBlock {\n    mat4 data;\n};\nuniform float scale;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(!result.contains("layout(location=0) uniform MyBlock"));
    assert!(
        result.contains("layout(std140, binding=1) uniform MyBlock"),
        "MyBlock should get binding=1, got: {}",
        result
    );
    assert!(result.contains("float scale;"), "scale should be packed into UBO, got: {}", result);
    assert!(
        !result.contains("layout(location="),
        "location should not be injected, got: {}",
        result
    );
}

#[test]
fn preprocess_skips_existing_layout_on_uniform() {
    // 带 layout(location) 前缀的 uniform 也被包装进 UBO 且 location 剥离
    // （gap 修复：旧 UNIFORM_RE 只匹配行首 uniform，带前缀行逃逸包装，
    //   Vulkan target 下 shaderc 编译失败）
    let src = "#version 330\nlayout(location=3) uniform mat4 MVP;\nuniform float scale;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        !result.contains("layout(location=3)"),
        "existing location should be stripped, got: {}",
        result
    );
    assert!(result.contains("mat4 MVP;"), "MVP should be packed into UBO, got: {}", result);
    assert!(result.contains("float scale;"), "scale should be packed into UBO, got: {}", result);
    assert!(
        !result.contains("layout(location="),
        "no location should remain, got: {}",
        result
    );
}

// ============ preprocess: UBO/SSBO binding 注入 ============

#[test]
fn preprocess_injects_binding_for_layout_ubo() {
    let src = "#version 330\nlayout(std140) uniform DynamicTransforms {\n    mat4 ModelViewMat;\n};\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.contains("layout(std140, binding=0) uniform DynamicTransforms"),
        "got: {}",
        result
    );
}

#[test]
fn preprocess_injects_binding_for_plain_ubo() {
    let src = "#version 330\nuniform MyBlock {\n    mat4 data;\n};\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("layout(std140, binding=0) uniform MyBlock"));
}

#[test]
fn preprocess_injects_binding_for_ssbo() {
    let src = "#version 430\nbuffer MyBuffer {\n    vec4 data[];\n};\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(
        result.contains("layout(std430, binding=0) buffer MyBuffer"),
        "got: {}",
        result
    );
}

#[test]
fn preprocess_skips_existing_binding_on_ubo() {
    let src = "#version 330\nlayout(std140, binding=3) uniform MyBlock {\n    mat4 data;\n};\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("layout(std140, binding=3) uniform MyBlock"));
}

#[test]
fn preprocess_assigns_incrementing_bindings() {
    let src = "#version 330\nlayout(std140) uniform Block1 {\n    mat4 a;\n};\nlayout(std140) uniform Block2 {\n    mat4 b;\n};\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("layout(std140, binding=0) uniform Block1"));
    assert!(result.contains("layout(std140, binding=1) uniform Block2"));
}

#[test]
fn preprocess_upgrades_version_when_binding_injected() {
    // #version 330 + UBO 注入 binding → 升级到 420+
    let src =
        "#version 330\nlayout(std140) uniform MyBlock {\n    mat4 data;\n};\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    // 升级到 450（force_glsl_version 已统一升级到 450）
    assert!(result.starts_with("#version 450 core"));
}

// ============ preprocess: 组合场景 ============

#[test]
fn preprocess_full_minecraft_style_vertex_shader() {
    let src = "#version 330\nlayout(std140) uniform DynamicTransforms {\n    mat4 ModelViewMat;\n    vec4 ColorModulator;\n};\nin vec3 Position;\nin vec4 Color;\nout vec4 vertexColor;\nvoid main() {\n    gl_Position = ModelViewMat * vec4(Position, 1.0);\n    vertexColor = Color;\n}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    // 版本升级到 450
    assert!(result.starts_with("#version 450 core"));
    // UBO 有 binding
    assert!(result.contains("layout(std140, binding=0) uniform DynamicTransforms"));
    // in/out 有 location，in/out 独立计数（都从 0 开始）
    assert!(result.contains("layout(location=0) in vec3 Position;"));
    assert!(result.contains("layout(location=1) in vec4 Color;"));
    assert!(result.contains("layout(location=0) out vec4 vertexColor;"));
}

#[test]
fn preprocess_full_minecraft_style_fragment_shader() {
    let src = "#version 330\nlayout(std140) uniform DynamicTransforms {\n    vec4 ColorModulator;\n};\nin vec4 vertexColor;\nout vec4 fragColor;\nvoid main() {\n    vec4 color = vertexColor;\n    if (color.a == 0.0) discard;\n    fragColor = color * ColorModulator;\n}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.starts_with("#version 450 core"));
    assert!(result.contains("layout(std140, binding=0) uniform DynamicTransforms"));
    assert!(result.contains("layout(location=0) in vec4 vertexColor;"));
    assert!(result.contains("layout(location=0) out vec4 fragColor;"));
}

#[test]
fn preprocess_preserves_shader_body() {
    let src = "#version 330\nvoid main() {\n    gl_Position = vec4(1.0);\n}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("void main() {"));
    assert!(result.contains("gl_Position = vec4(1.0);"));
    assert!(result.contains("}"));
}

#[test]
fn preprocess_handles_empty_input() {
    let result = preprocess::preprocess("", 0x8B31);
    // 空输入应插入 #version 450 core
    assert!(result.starts_with("#version 450 core"));
}

#[test]
fn preprocess_preserves_450_shader_with_full_layouts() {
    // 已有完整 layout 的 450 shader 不应被修改 layout
    let src = "#version 450 core\nlayout(location=0) in vec3 pos;\nlayout(location=0) out vec4 color;\nlayout(binding=0) uniform sampler2D tex;\nvoid main() {\n    color = texture(tex, pos.xy);\n}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("layout(location=0) in vec3 pos;"));
    assert!(result.contains("layout(location=0) out vec4 color;"));
    assert!(result.contains("layout(binding=0) uniform sampler2D tex;"));
}

#[test]
fn preprocess_handles_compute_shader_style() {
    // compute shader 无 in/out 变量
    let src = "#version 450 core\nlayout(local_size_x=8, local_size_y=8) in;\nvoid main() {}\n";
    let result = preprocess::preprocess(src, 0x8B31);
    assert!(result.contains("layout(local_size_x=8, local_size_y=8) in;"));
}
