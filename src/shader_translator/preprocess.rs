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
/// - #version < 330 且非 ES 版本 → 升级到 330 core
///   桌面 GLSL 100-150 使用旧语法（attribute/varying/gl_FragColor），
///   升级到 330 core 后 glslang 可能仍因旧语法拒绝，但部分已用 in/out 的 shader 能通过。
/// - #version < 330 且 ES 版本 → 保持不变（GLSL ES 语法与桌面不兼容，升级无意义）
/// - #version 330-450 且非 ES → 升级到 450 core
///   （含 compatibility profile：glslang 在 OpenGL SPIR-V 模式拒绝 Compatibility，
///   统一替换为 450 core 去除 compatibility）
/// - #version 460 且非 ES → 保持 460（460 是 450 超集，降级会丢失 subgroup 等特性），
///   但规范化为 core profile（移除 compatibility）
/// - ES 版本 → 保持不变
fn force_glsl_version(result: &mut String) {
    let version = extract_version(result);
    match version {
        None => {
            result.insert_str(0, "#version 450 core\n");
        }
        Some(v) => {
            if let Some(ver) = parse_version_number(v) {
                let is_es = is_es_version(v);
                if ver >= 330 && ver < 460 && !is_es {
                    // 330-450 桌面版本统一升级到 450 core
                    // 含 compatibility profile 的也会被替换为 core（glslang 拒绝 Compatibility）
                    let re = Regex::new(r"(?m)^#version\s+\d+.*$").unwrap();
                    *result = re.replace(result, "#version 450 core").to_string();
                } else if ver == 460 && !is_es {
                    // 460 保持版本号，仅规范化为 core（移除 compatibility profile）
                    // glslang OpenGL SPIR-V 模式拒绝 Compatibility，460 compatibility 需替换为 core
                    let re = Regex::new(r"(?m)^#version\s+\d+.*$").unwrap();
                    *result = re.replace(result, "#version 460 core").to_string();
                } else if ver < 330 && !is_es {
                    // 桌面 GLSL 旧版本升级到 330 core（OpenGL SPIR-V 最低要求）
                    // ES 版本保持不变（语法不兼容，升级无意义）
                    let re = Regex::new(r"(?m)^#version\s+\d+.*$").unwrap();
                    *result = re.replace(result, "#version 330 core").to_string();
                }
                // ES 版本: 保持不变
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

/// 判断 #version 行是否为 ES 版本
///
/// 精确匹配 "es" 作为独立 token（大小写不敏感）。
/// 之前用 `contains("es")` 会误匹配 meshes/textures/harness/entities 等含 "es" 子串的
/// 注释或文本，导致 ES 误判、跳过版本升级，进而破坏后续 layout 限定符注入。
fn is_es_version(version_line: &str) -> bool {
    version_line
        .split_whitespace()
        .any(|t| t.eq_ignore_ascii_case("es"))
}

/// 从 layout 限定符字符串中解析 binding 值
/// 输入如 "std140, binding=3" 或 "binding = 5" → 返回 3 或 5
fn parse_binding(qualifiers: &str) -> Option<u32> {
    let re = Regex::new(r"binding\s*=\s*(\d+)").unwrap();
    re.captures(qualifiers)
        .and_then(|c| c[1].parse::<u32>().ok())
}

/// 为缺少 location 的 in/out 变量自动添加 layout(location=X)
///
/// OpenGL SPIR-V 模式下，glslang parse 阶段要求所有 in/out 变量必须有 location
/// （AUTO_MAP_LOCATIONS 在 parse 之后才生效，来不及自动分配）。
///
/// 关键修复：
/// 1. in 和 out 使用独立的 counter，分别从 0 开始（不同接口空间，不冲突）
/// 2. 正则匹配插值修饰符（flat/smooth/noperspective/centroid/patch/invariant），
///    避免漏注入 `flat in vec3 normal;` 等常见声明
/// 3. 已有 layout(location=N) 的声明会推进 counter 到 N+1（而非跳过不推进），
///    避免后续注入的 location 与已有值冲突
/// 4. 数组声明 `in vec4 arr[N];` 按 N 推进 counter（占 N 个 location）
fn inject_missing_locations(result: &mut String) {
    // 匹配带可选前导 layout 和插值修饰符的 in/out 声明
    // 情况1: [layout(...)] [修饰符] in/out type name[;];
    // 情况2: [修饰符] in/out type name;
    let re = Regex::new(
        r"(?m)^(?P<indent>\s*)(?:layout\s*\(\s*(?P<layout_qual>[^)]*)\s*\)\s*)?(?P<prefix>(?:(?:flat|smooth|noperspective|centroid|patch|precise|invariant)\s+)*)(?P<qualifier>in|out)\s+(?P<rest>.+?;[^\n]*)$"
    ).unwrap();

    // in 和 out 独立计数，分别从 0 开始（不同接口空间，不冲突）
    let mut in_counter: u32 = 0;
    let mut out_counter: u32 = 0;
    let mut modified = String::with_capacity(result.len());

    for line in result.lines() {
        if let Some(caps) = re.captures(line) {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let layout_qual = caps.name("layout_qual").map(|m| m.as_str()).unwrap_or("");
            let prefix = caps.name("prefix").map(|m| m.as_str()).unwrap_or("");
            let qualifier = caps.name("qualifier").map(|m| m.as_str()).unwrap_or("");
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");

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

            // 检查是否已有 location
            if layout_qual.contains("location") {
                // 已有 location：解析值并推进 counter 到 max(counter, location + array_size)
                if let Some(loc) = parse_layout_location(layout_qual) {
                    let array_size = parse_array_size(rest);
                    *counter = (*counter).max(loc + array_size);
                }
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 注入 layout(location=N)，保留前导修饰符
            let new_line = format!(
                "{}layout(location={}) {}{} {}",
                indent, counter, prefix, qualifier, rest
            );
            modified.push_str(&new_line);
            modified.push('\n');
            // 数组声明占多个 location
            *counter += parse_array_size(rest);
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

/// 从 layout 限定符字符串中解析 location 值
/// 输入如 "std140, location=3" 或 "location = 5" → 返回 3 或 5
fn parse_layout_location(layout_qual: &str) -> Option<u32> {
    let re = Regex::new(r"location\s*=\s*(\d+)").unwrap();
    re.captures(layout_qual)
        .and_then(|c| c[1].parse::<u32>().ok())
}

/// 解析变量声明中的数组大小（用于计算 location 占用数）
/// 输入如 "vec4 colors[4];" → 返回 4，"vec4 color;" → 返回 1
fn parse_array_size(rest: &str) -> u32 {
    if let Some(bracket_start) = rest.find('[') {
        if let Some(bracket_end) = rest[bracket_start..].find(']') {
            let size_str = &rest[bracket_start + 1..bracket_start + bracket_end];
            if let Ok(size) = size_str.trim().parse::<u32>() {
                return size.max(1);
            }
        }
    }
    1
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
    // 包含整数/无符号变体：isampler/usampler/itexture/utexture/iimage/uimage
    const OPAQUE_PREFIXES: &[&str] = &[
        "sampler",
        "isampler",
        "usampler",
        "texture",
        "itexture",
        "utexture",
        "image",
        "iimage",
        "uimage",
        "atomic_uint",
        "subpass",
    ];

    let mut location_counter: u32 = 0;
    let mut modified = String::with_capacity(result.len());

    for line in result.lines() {
        if let Some(caps) = re.captures(line) {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let type_name = caps.name("type").map(|m| m.as_str()).unwrap_or("");
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");

            // 跳过已有 location 的声明（仅检查 location，避免误跳过有 layout(std140)
            // 等非 location 限定符但缺 location 的 standalone uniform，导致 glslang parse 失败）
            if line.contains("location") {
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
            // 数组声明占多个 location
            location_counter += parse_array_size(rest);
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
                // 已有 binding：解析值并推进 counter 到 max(counter, existing+1)，
                // 避免后续注入的 binding 与已有值冲突。
                if let Some(existing) = parse_binding(qualifiers) {
                    binding_counter = binding_counter.max(existing + 1);
                }
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 注入 binding。保留 `{` 之后的内容（单行 block 如
            // `layout(std140) uniform Block { mat4 m; } inst;` 不丢失字段与实例名）。
            let trailing = caps.get(0).map(|m| &line[m.end()..]).unwrap_or("");
            let new_qualifiers = if qualifiers.trim().is_empty() {
                format!("binding={}", binding_counter)
            } else {
                format!("{}, binding={}", qualifiers.trim(), binding_counter)
            };
            let new_line = format!(
                "{}layout({}) {} {} {{{}",
                indent, new_qualifiers, kind, name, trailing
            );
            modified.push_str(&new_line);
            modified.push('\n');
            binding_counter += 1;
            injected = true;
        } else if let Some(caps) = re_plain_block.captures(line) {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let kind = caps.name("kind").map(|m| m.as_str()).unwrap_or("uniform");
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or("");

            // 注入 layout(std140, binding=N) 或 layout(std430, binding=N)
            // 保留 `{` 之后的内容（同上，单行 block 不丢失字段与实例名）
            let trailing = caps.get(0).map(|m| &line[m.end()..]).unwrap_or("");
            let layout_qualifier = if kind == "buffer" {
                format!("std430, binding={}", binding_counter)
            } else {
                format!("std140, binding={}", binding_counter)
            };
            let new_line = format!(
                "{}layout({}) {} {} {{{}",
                indent, layout_qualifier, kind, name, trailing
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
///
/// 注意：ES 版本（300 es/310 es/320 es）已支持 layout(binding)，无需升级。
/// 之前不检查 is_es，导致 `#version 320 es` 被错误替换为 `#version 420`（桌面），
/// 破坏 ES 语法。
fn ensure_binding_version(result: &mut String) {
    let need_upgrade = extract_version(result).and_then(|v| {
        let is_es = is_es_version(v);
        if is_es {
            // ES 310+ 已支持 layout(binding)，无需升级
            return None;
        }
        parse_version_number(v).map(|ver| ver < 420)
    });

    if !need_upgrade.unwrap_or(false) {
        return;
    }

    let re = Regex::new(r"(?m)^#version\s+\d+.*$").unwrap();
    *result = re.replace(result, "#version 420 core").to_string();
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
        // 桌面 GLSL < 330 升级到 330 core（OpenGL SPIR-V 最低要求）
        let mut result = "#version 120\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 330 core"));
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
