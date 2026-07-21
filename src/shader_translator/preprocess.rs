//! GLSL 源码预处理模块
//!
//! 对齐 MobileGlues 的 preprocess_glsl，并额外处理 SPIR-V 编译所需的
//! location/binding 自动分配（作为安全网，OpenGL SPIR-V 模式下非必需）。
//!
//! 使用 OpenGL SPIR-V 目标（client=OpenGL, target=SPIRV）编译 GLSL，
//! 该模式比 Vulkan 目标更宽松：
//! - 允许独立 non-opaque uniform（无需包装进 UBO block）
//! - 允许省略 layout(location) 和 layout(binding)
//! - 要求 GLSL >= 330
//!
//! 重要：in/out varying 的 location 注入使用独立的 in_counter 和 out_counter。
//! 之前的实现用单一 counter，导致 VS in(location=0) 和 VS out(location=0) 被视为
//! 冲突，spirv-cross 重新分配 VS out 到高 location，而 FS in 保留低 location，
//! 导致跨 stage 链接失败（output X location mismatch）。
//! 修复：in 和 out 是独立的接口空间（SPIR-V Input/Output StorageClass），
//! 分别从 0 计数，保证 VS out 和 FS in 的同名 varying location 一致。
//!
//! 仍会注入：
//! - non-opaque uniform 的 layout(location)（OpenGL SPIR-V 模式要求，否则 parse 失败）
//! - UBO/SSBO 的 layout(binding)（作为安全网，配合 AUTO_MAP_BINDINGS）

use regex::Regex;

/// GLSL 预处理主入口
///
/// 执行顺序：
/// 1. 移除 #line 指令
/// 2. 强制 GLSL 版本（无版本插入 450，330-440 升级到 450）
/// 3. 为缺少 location 的 in/out 变量自动添加 layout(location=X)（in/out 独立计数）
/// 4. 为缺少 location 的 non-opaque uniform 自动添加 layout(location=X)
/// 5. 为缺少 binding 的 UBO/SSBO 自动添加 layout(binding=X)
/// 6. 如果注入了 binding 且版本低于 420，升级到 420（binding 需要 GLSL 420+）
pub fn preprocess(source: &str) -> String {
    let mut result = remove_line_directives(source);
    force_glsl_version(&mut result);
    inject_missing_locations(&mut result);
    inject_missing_uniform_locations(&mut result);
    let injected_binding = inject_missing_bindings(&mut result);
    if injected_binding {
        ensure_binding_version(&mut result);
    }
    result
}

/// 提取 GLSL 源码中的 #version 行
pub fn extract_version(source: &str) -> Option<&str> {
    source
        .lines()
        .find(|l| l.trim_start().starts_with("#version"))
}

/// 移除 #line 指令（对齐 MobileGlues replace_line_starting_with("#line")）
fn remove_line_directives(source: &str) -> String {
    let re = Regex::new(r"(?m)^\s*#line\s+.*$(\n|$)?").unwrap();
    re.replace_all(source, "").to_string()
}

/// 确保 GLSL 版本满足 OpenGL SPIR-V 要求并支持 layout 限定符
///
/// - 无 #version → 插入 #version 450 core
/// - #version < 330 → 保持不变（glslang 会拒绝，OpenGL SPIR-V 要求 330+）
/// - #version 330-440 → 升级到 450（layout(location) for uniform 需要 420+，
///   layout(binding) 需要 420+，统一升级到 450 避免版本碎片化）
/// - #version >= 450 → 保持不变
fn force_glsl_version(result: &mut String) {
    let version = extract_version(result);
    match version {
        None => {
            result.insert_str(0, "#version 450 core\n");
        }
        Some(v) => {
            if let Some(ver) = parse_version_number(v) {
                if ver >= 330 && ver < 450 {
                    let re = Regex::new(r"(?m)^#version\s+\d+.*$").unwrap();
                    *result = re.replace(result, "#version 450 core").to_string();
                }
                // ver < 330: 保持不变，glslang 会拒绝
                // ver >= 450: 保持不变
            }
        }
    }
}

/// 从 #version 行中解析版本号
fn parse_version_number(version_line: &str) -> Option<u32> {
    version_line
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<u32>().ok())
}

/// 为缺少 location 的 in/out 变量自动添加 layout(location=X)
///
/// OpenGL SPIR-V 模式下，glslang parse 阶段要求所有 in/out 变量必须有 location
/// （AUTO_MAP_LOCATIONS 在 parse 之后才生效，来不及自动分配）。
///
/// 关键：in 和 out 使用独立的 counter，分别从 0 开始。
/// in 和 out 是不同的接口空间（SPIR-V Input/Output StorageClass），
/// 同一个 shader 内 in(location=0) 和 out(location=0) 不冲突。
/// 这样 VS out 和 FS in 的同名 varying 都从 0 开始分配，保证跨 stage 一致。
///
/// 之前用单一 counter 导致 VS in(已有 location=0) 和 VS out(注入 location=0) 被视为
/// 冲突，spirv-cross 重新分配 VS out 到高 location，破坏跨 stage 链接。
fn inject_missing_locations(result: &mut String) {
    let re = Regex::new(r"(?m)^(?P<indent>\s*)(?P<qualifier>in|out)\s+(?P<rest>.+?;\s*)$").unwrap();

    // in 和 out 独立计数，分别从 0 开始（不同接口空间，不冲突）
    let mut in_counter: u32 = 0;
    let mut out_counter: u32 = 0;
    let mut modified = String::with_capacity(result.len());

    for line in result.lines() {
        if let Some(caps) = re.captures(line) {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let qualifier = caps.name("qualifier").map(|m| m.as_str()).unwrap_or("");
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");

            // 跳过已有 layout(location=...) 的声明
            if rest.contains("layout(") && rest.contains("location") {
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 跳过 block 声明（含 { 的单行）
            if rest.contains('{') {
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 按 in/out 选择独立 counter
            let counter = if qualifier == "in" {
                &mut in_counter
            } else {
                &mut out_counter
            };
            let new_line = format!(
                "{}layout(location={}) {} {}",
                indent, counter, qualifier, rest
            );
            modified.push_str(&new_line);
            modified.push('\n');
            *counter += 1;
        } else {
            modified.push_str(line);
            modified.push('\n');
        }
    }

    // 移除末尾多余的换行
    if modified.ends_with('\n') && !result.ends_with('\n') {
        modified.pop();
    }

    *result = modified;
}

/// 为缺少 location 的 non-opaque uniform 变量自动添加 layout(location=X)
///
/// OpenGL SPIR-V 模式要求所有 non-opaque uniform（mat4, vec3, float 等）
/// 必须有 layout(location=L)，否则 glslang parse 阶段报错：
/// "non-opaque uniform variables need a layout(location=L)"
///
/// opaque uniform（sampler/texture/image）由 AUTO_MAP_BINDINGS 处理，不需要此注入。
/// uniform block 由 inject_missing_bindings 处理。
///
/// 策略：扫描独立的 `uniform <type> <name>;` 声明（非 block、非 opaque），
/// 为缺少 location 的声明按出现顺序分配递增 location 编号。
fn inject_missing_uniform_locations(result: &mut String) {
    // 匹配: uniform <type> <name>[<array>];
    // Rust regex crate 不支持 lookahead，opaque 类型在代码中过滤
    let re =
        Regex::new(r"(?m)^(?P<indent>\s*)uniform\s+(?P<type>\w+)\s+(?P<rest>.+?;\s*)$").unwrap();

    // opaque 类型前缀：这些类型由 AUTO_MAP_BINDINGS 处理，不需要 layout(location)
    const OPAQUE_PREFIXES: &[&str] = &["sampler", "texture", "image", "atomic_uint", "subpass"];

    let mut location_counter: u32 = 0;
    let mut modified = String::with_capacity(result.len());

    for line in result.lines() {
        if let Some(caps) = re.captures(line) {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let type_name = caps.name("type").map(|m| m.as_str()).unwrap_or("");
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");

            // 跳过已有 layout 的声明
            if line.contains("layout(") {
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 跳过 block 声明（包含 {）
            if rest.contains('{') {
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 跳过 opaque 类型（sampler/texture/image/atomic_uint/subpass）
            if OPAQUE_PREFIXES.iter().any(|p| type_name.starts_with(p)) {
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 注入 layout(location=N)
            let new_line = format!(
                "{}layout(location={}) uniform {} {}",
                indent, location_counter, type_name, rest
            );
            modified.push_str(&new_line);
            modified.push('\n');
            location_counter += 1;
        } else {
            modified.push_str(line);
            modified.push('\n');
        }
    }

    // 移除末尾多余的换行
    if modified.ends_with('\n') && !result.ends_with('\n') {
        modified.pop();
    }

    *result = modified;
}

/// 为缺少 binding 的 UBO/SSBO 自动添加 layout(binding=X)
///
/// Vulkan SPIR-V 要求所有 uniform buffer 和 shader storage buffer 必须有 binding。
/// 桌面 GLSL 允许省略 binding（由链接器分配），但 Vulkan 不允许。
///
/// 策略：扫描 `layout(...) uniform/buffer Name {...}` 和 `uniform block Name {...}`，
/// 为缺少 binding 的声明按出现顺序分配递增 binding 编号。
///
/// 返回 true 表示至少注入了一个 binding（调用方可能需要升级 GLSL 版本）。
fn inject_missing_bindings(result: &mut String) -> bool {
    // 匹配 layout(...) uniform/buffer 块声明
    // 例如：
    //   layout(std140) uniform DynamicTransforms {
    //   layout(std430) buffer MyBuffer {
    //   uniform MyBlock {
    let re_layout_block = Regex::new(
        r"(?m)^(?P<indent>\s*)layout\s*\(\s*(?P<qualifiers>[^)]*)\s*\)\s*(?P<kind>uniform|buffer)\s+(?P<name>\w+)\s*\{",
    )
    .unwrap();

    // 匹配无 layout 的 uniform/buffer 块声明
    // 例如：
    //   uniform MyBlock {
    let re_plain_block =
        Regex::new(r"(?m)^(?P<indent>\s*)(?P<kind>uniform|buffer)\s+(?P<name>\w+)\s*\{").unwrap();

    let mut binding_counter: u32 = 0;
    let mut injected = false;
    let mut modified = String::with_capacity(result.len());

    for line in result.lines() {
        // 先检查带 layout 的块
        if let Some(caps) = re_layout_block.captures(line) {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let qualifiers = caps.name("qualifiers").map(|m| m.as_str()).unwrap_or("");
            let kind = caps.name("kind").map(|m| m.as_str()).unwrap_or("uniform");
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or("");

            // 检查是否已有 binding
            if qualifiers.contains("binding") {
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 注入 binding
            let new_qualifiers = if qualifiers.trim().is_empty() {
                format!("binding={}", binding_counter)
            } else {
                format!("{}, binding={}", qualifiers.trim(), binding_counter)
            };
            let new_line = format!("{}layout({}) {} {} {{", indent, new_qualifiers, kind, name);
            modified.push_str(&new_line);
            modified.push('\n');
            binding_counter += 1;
            injected = true;
        } else if let Some(caps) = re_plain_block.captures(line) {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let kind = caps.name("kind").map(|m| m.as_str()).unwrap_or("uniform");
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or("");

            // 注入 layout(std140, binding=N) 或 layout(std430, binding=N)
            let layout_qualifier = if kind == "buffer" {
                format!("std430, binding={}", binding_counter)
            } else {
                format!("std140, binding={}", binding_counter)
            };
            let new_line = format!(
                "{}layout({}) {} {} {{",
                indent, layout_qualifier, kind, name
            );
            modified.push_str(&new_line);
            modified.push('\n');
            binding_counter += 1;
            injected = true;
        } else {
            modified.push_str(line);
            modified.push('\n');
        }
    }

    // 移除末尾多余的换行
    if modified.ends_with('\n') && !result.ends_with('\n') {
        modified.pop();
    }

    *result = modified;
    injected
}

/// 如果 GLSL 版本低于 420，升级到 420
///
/// `layout(binding=X)` 需要 GLSL 420+，否则 glslang 会报：
/// "binding : not supported for this version or the enabled extensions"
fn ensure_binding_version(result: &mut String) {
    let need_upgrade = extract_version(result)
        .and_then(parse_version_number)
        .map(|v| v < 420)
        .unwrap_or(true);

    if !need_upgrade {
        return;
    }

    let re = Regex::new(r"(?m)^#version\s+\d+.*$").unwrap();
    *result = re.replace(result, "#version 420").to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_line_directives() {
        let input = "#version 330\n#line 0 2\nvoid main() {}\n";
        let result = remove_line_directives(input);
        assert!(!result.contains("#line"));
    }

    #[test]
    fn test_force_glsl_version_none() {
        let mut result = "void main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 450 core"));
    }

    #[test]
    fn test_force_glsl_version_keep_low() {
        // #version < 330 保持不变（glslang 会拒绝）
        let mut result = "#version 120\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 120"));
    }

    #[test]
    fn test_force_glsl_version_upgrade_330_to_450() {
        // #version 330-440 升级到 450
        let mut result = "#version 330\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 450 core"));
    }

    #[test]
    fn test_force_glsl_version_keep_450() {
        // #version >= 450 保持不变
        let mut result = "#version 450\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 450"));
    }

    #[test]
    fn test_inject_locations_independent_counters() {
        // in 和 out 应使用独立 counter，分别从 0 开始
        let mut result =
            "#version 450\nin vec4 color;\nout vec4 fragColor;\nvoid main() {}\n".to_string();
        inject_missing_locations(&mut result);
        // in 从 0 开始
        assert!(result.contains("layout(location=0) in vec4 color;"));
        // out 也从 0 开始（独立计数，不与 in 冲突）
        assert!(result.contains("layout(location=0) out vec4 fragColor;"));
    }

    #[test]
    fn test_inject_locations_skip_existing_independent() {
        // 已有 location 的声明跳过，但 in/out counter 独立推进
        let mut result =
            "#version 450\nlayout(location=5) in vec4 color;\nout vec4 fragColor;\n".to_string();
        inject_missing_locations(&mut result);
        // 已有 location=5 的 in 不变
        assert!(result.contains("layout(location=5) in vec4 color;"));
        // out 缺少 location，从 0 开始（独立于 in 的已有 location）
        assert!(result.contains("layout(location=0) out vec4 fragColor;"));
    }

    #[test]
    fn test_inject_locations_vs_in_no_conflict() {
        // 模拟 MC 场景：VS in 已有 location，VS out 无 location
        // 修复前（单一 counter）：out 被注入 location=0，与 in(location=0) 冲突
        // 修复后（独立 counter）：out 被注入 location=0，与 in(location=0) 不冲突（不同接口空间）
        let mut result = "#version 450\n\
            layout(location=0) in vec3 Position;\n\
            layout(location=1) in vec4 Color;\n\
            out vec4 vertexColor;\n\
            void main() { gl_Position = vec4(Position, 1.0); vertexColor = Color; }\n"
            .to_string();
        inject_missing_locations(&mut result);
        // in 保持已有 location
        assert!(result.contains("layout(location=0) in vec3 Position;"));
        assert!(result.contains("layout(location=1) in vec4 Color;"));
        // out 从 0 开始（独立 counter，与 in 不冲突）
        assert!(result.contains("layout(location=0) out vec4 vertexColor;"));
    }

    #[test]
    fn test_inject_uniform_locations() {
        let mut result =
            "#version 450\nuniform mat4 MVP;\nuniform vec3 color;\nvoid main() {}\n".to_string();
        inject_missing_uniform_locations(&mut result);
        assert!(result.contains("layout(location=0) uniform mat4 MVP;"));
        assert!(result.contains("layout(location=1) uniform vec3 color;"));
    }

    #[test]
    fn test_inject_uniform_locations_skip_opaque() {
        // sampler/texture/image 是 opaque，不应注入 location
        let mut result = "#version 450\nuniform sampler2D tex;\nuniform mat4 MVP;\n".to_string();
        inject_missing_uniform_locations(&mut result);
        assert!(!result.contains("layout(location=0) uniform sampler2D"));
        assert!(result.contains("layout(location=0) uniform mat4 MVP;"));
    }

    #[test]
    fn test_inject_uniform_locations_skip_block() {
        // uniform block 不应注入 location
        let mut result =
            "#version 450\nuniform MyBlock {\n    mat4 data;\n};\nuniform float scale;\n"
                .to_string();
        inject_missing_uniform_locations(&mut result);
        assert!(!result.contains("layout(location=0) uniform MyBlock"));
        assert!(result.contains("layout(location=0) uniform float scale;"));
    }

    #[test]
    fn test_inject_uniform_locations_skip_existing_layout() {
        let mut result =
            "#version 450\nlayout(location=3) uniform mat4 MVP;\nuniform float scale;\n"
                .to_string();
        inject_missing_uniform_locations(&mut result);
        assert!(result.contains("layout(location=3) uniform mat4 MVP;"));
        assert!(result.contains("layout(location=0) uniform float scale;"));
    }

    #[test]
    fn test_inject_bindings_layout_block() {
        let mut result = "#version 330\nlayout(std140) uniform DynamicTransforms {\n    mat4 ModelViewMat;\n};\n".to_string();
        let injected = inject_missing_bindings(&mut result);
        assert!(injected);
        assert!(result.contains("layout(std140, binding=0) uniform DynamicTransforms"));
    }

    #[test]
    fn test_inject_bindings_plain_block() {
        let mut result = "#version 330\nuniform MyBlock {\n    mat4 data;\n};\n".to_string();
        let injected = inject_missing_bindings(&mut result);
        assert!(injected);
        assert!(result.contains("layout(std140, binding=0) uniform MyBlock"));
    }

    #[test]
    fn test_inject_bindings_skip_existing() {
        let mut result =
            "#version 330\nlayout(std140, binding=3) uniform MyBlock {\n    mat4 data;\n};\n"
                .to_string();
        let injected = inject_missing_bindings(&mut result);
        assert!(!injected);
        assert!(result.contains("layout(std140, binding=3) uniform MyBlock"));
    }

    #[test]
    fn test_ensure_binding_version_upgrade() {
        let mut result = "#version 330\nvoid main() {}".to_string();
        ensure_binding_version(&mut result);
        assert!(result.starts_with("#version 420"));
    }

    #[test]
    fn test_ensure_binding_version_ok() {
        let mut result = "#version 450\nvoid main() {}".to_string();
        ensure_binding_version(&mut result);
        assert!(result.starts_with("#version 450"));
    }

    #[test]
    fn test_preprocess_full_pipeline() {
        let input = "#version 330\nlayout(std140) uniform MyBlock {\n    mat4 data;\n};\nin vec4 color;\nout vec4 fragColor;\nuniform mat4 MVP;\nvoid main() {\n    fragColor = color;\n}\n";
        let result = preprocess(input);
        // 版本应升级到 450
        assert!(result.contains("#version 450 core"));
        // UBO 应有 binding
        assert!(result.contains("layout(std140, binding=0) uniform MyBlock"));
        // non-opaque uniform 应有 location
        assert!(result.contains("layout(location=0) uniform mat4 MVP;"));
        // in/out 应有 location，且 in/out 独立计数（都从 0 开始）
        assert!(result.contains("layout(location=0) in vec4 color;"));
        assert!(result.contains("layout(location=0) out vec4 fragColor;"));
    }
}
