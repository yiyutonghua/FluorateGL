//! GLSL 源码预处理模块
//!
//! 本分支（glslang-targetvk）使用 Vulkan target 编译 GLSL → SPIR-V。
//! Vulkan target 要求 GLSL >= 140，#version 150 可直接处理，无需版本升级。
//! Vulkan 拒绝独立 non-opaque uniform（必须包装进 UBO），因此不再注入
//! uniform 的 layout(location)。
//!
//! 保留的注入：
//! - in/out varying 的 layout(location)（Vulkan 要求所有 in/out 有 location）
//! - UBO/SSBO 的 layout(binding)（Vulkan 要求所有 buffer 有 binding）
//!
//! 重要：in/out varying 的 location 注入使用独立的 in_counter 和 out_counter。
//! in 和 out 是独立的接口空间（SPIR-V Input/Output StorageClass），
//! 分别从 0 计数，保证 VS out 和 FS in 的同名 varying location 一致。

use regex::Regex;
use std::collections::HashSet;

/// GLSL 预处理主入口
///
/// 执行顺序：
/// 1. 移除 #line 指令
/// 2. 移除 MC 的 `/*#version N*\/` 注释（moj_import 文本拼接产物，会干扰 glslang parse）
/// 3. 规范化 GLSL 版本（无版本插入 450，< 140 升级到 140，移除 compatibility profile）
/// 4. 为缺少 location 的 in/out 变量自动添加 layout(location=X)（in/out 独立计数）
/// 5. 为缺少 binding 的 UBO/SSBO 自动添加 layout(binding=X)
pub fn preprocess(source: &str) -> String {
    let mut result = remove_line_directives(source);
    strip_mc_version_comment(&mut result);
    force_glsl_version(&mut result);
    rename_vulkan_builtin_variables(&mut result);
    // 新增：转换独立 uniform 到 UBO
    result = convert_uniforms_to_ubo(&result);
    inject_missing_locations(&mut result);
    inject_missing_bindings(&mut result);
    result
}

/// 将桌面 GLSL 的内置变量重命名为 Vulkan GLSL 对应的名称
fn rename_vulkan_builtin_variables(result: &mut String) {
    // 1. gl_VertexID -> gl_VertexIndex
    let re_vertex = Regex::new(r"\bgl_VertexID\b").unwrap();
    *result = re_vertex.replace_all(result, "gl_VertexIndex").into_owned();

    // 2. 变量名 sampler -> u_sampler (避免与关键字冲突)
    // \b 保证了 sampler2D 中的 sampler 不会被替换
    let re_sampler = Regex::new(r"\bsampler\b").unwrap();
    let new_result = re_sampler.replace_all(result, "u_sampler").into_owned();
    if new_result.len() != result.len() {
        log::debug!("[ShaderTranslator] preprocess 重命名了变量 sampler -> u_sampler");
        *result = new_result;
    }
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

/// 移除 Minecraft moj_import 产生的 `/*#version N*/` 注释
///
/// MC 在 Java 端文本拼接 include 时会留下被注释掉的旧 #version 行，
/// 形如 `/*#version 330*/`。虽然对桌面 GLSL 是合法注释，但某些 glslang
/// 版本在 Vulkan target 下 parse 时会受其干扰（可能与 #version 检测逻辑
/// 冲突），导致静默失败。在 preprocess 阶段主动移除，与 string_pass 回退
/// 路径保持一致。
///
/// 正则与 string_pass::strip_mc_version_comment 保持一致。
fn strip_mc_version_comment(result: &mut String) {
    let re = Regex::new(r"/\*#version\s+\d+\s*\*/").unwrap();
    let new_result = re.replace_all(result, "").into_owned();
    if new_result.len() != result.len() {
        log::debug!(
            "[ShaderTranslator] preprocess 移除了 /*#version N*/ 注释 ({} -> {} chars)",
            result.len(),
            new_result.len()
        );
        *result = new_result;
    }
}

/// 规范化 GLSL 版本以满足 Vulkan target + layout 限定符要求
///
/// Vulkan target 要求 GLSL >= 140，且拒绝 compatibility profile。
/// preprocess 注入的 `layout(binding=)` 和 `layout(location=)` 需要 GLSL 420+
/// （GL_ARB_shading_language_420pack），否则 glslang parse 报
/// "not supported for this version or the enabled extensions"。
///
/// - 无 #version → 插入 #version 450 core
/// - #version < 460 且非 ES → 升级到 450 core（统一支持 layout 限定符 + 移除 compatibility）
/// - #version == 460 且非 ES → 保持 460，规范化为 core
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
                if is_es {
                    // ES 版本保持不变（语法不兼容，升级无意义）
                    return;
                }
                let re = Regex::new(r"(?m)^#version\s+\d+.*$").unwrap();
                if ver < 460 {
                    // 桌面 GLSL < 460 升级到 450 core
                    // （支持 layout(binding/location)，移除 compatibility，Vulkan 接受 >= 140）
                    *result = re.replace(result, "#version 450 core").to_string();
                } else {
                    // 460 保持版本号，仅规范化为 core
                    *result = re.replace(result, "#version 460 core").to_string();
                }
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

    // 初始 binding_counter 从已有 binding 的最大值+1 开始，避免与
    // convert_uniforms_to_ubo 注入的 UniformBlock 或原生带 binding 的块冲突。
    // 例如：UniformBlock 已占 binding=0，此处 counter 应从 1 开始，
    // 否则会给后续无 binding 的块分配 binding=0 造成链接冲突。
    let mut binding_counter: u32 = find_available_binding(result);
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

/// 将全局非不透明 uniform 转换为一个独立的 UBO，并替换所有引用。
///
/// Vulkan target 拒绝独立 non-opaque uniform（必须包装进 UBO）。
/// 本函数扫描单行 `uniform T name;` 声明，收集后包装进 `UniformBlock`，
/// 并把源码中对这些变量的引用替换为 `UniformBlock.name`。
///
/// 关键顺序（避免历史 Bug）：
/// 1. 先从原始 src 收集 uniform，同时产出删除了 uniform 行的 cleaned_src
/// 2. 在 cleaned_src 上替换变量引用 name → UniformBlock.name
/// 3. 最后构建 UBO 声明（成员名用原始 name）并插入到 #version 之后
///
/// 若先插入 UBO 声明再全局替换，会把 UBO 块内 `mat4 ModelViewMat;`
/// 错误替换成 `mat4 UniformBlock.ModelViewMat;`（成员名带点号，GLSL 非法）。
///
/// 注意：假定 uniform 名唯一且无同名局部变量/注释干扰。
fn convert_uniforms_to_ubo(src: &str) -> String {
    // 匹配单行全局 uniform 声明（不包括块，不包括 sampler/image/atomic_uint）
    // 例如：uniform mat4 ModelViewMat;
    //        uniform vec4 ColorModulator;
    let uniform_re = Regex::new(
        r"(?m)^\s*uniform\s+(?P<type>[a-zA-Z_][\w]*(\s*\*\s*)?(?:[a-zA-Z_][\w]*\s*)?)\s+(?P<name>[a-zA-Z_][\w]*)\s*;"
    ).unwrap();

    let mut uniforms = Vec::new();
    // cleaned_lines 收集非 uniform 行（uniform 声明行被删除）
    let mut cleaned_lines = Vec::new();

    for line in src.lines() {
        if let Some(caps) = uniform_re.captures(line) {
            let ty = caps.name("type").unwrap().as_str();
            let name = caps.name("name").unwrap().as_str();
            // 排除不透明类型：sampler, image, atomic_uint, buffer（不透明）
            if !ty.contains("sampler")
                && !ty.contains("image")
                && !ty.contains("atomic_uint")
                && !ty.contains("buffer")
            {
                uniforms.push((ty.to_string(), name.to_string()));
                continue; // 删除此行（不加入 cleaned_lines）
            }
        }
        cleaned_lines.push(line);
    }

    if uniforms.is_empty() {
        return src.to_string(); // 无变化
    }

    // Bug 2 修复：用 cleaned_lines（已删除 uniform 行）拼成 cleaned_src，
    // 而非用原始 src（否则原始 uniform 声明行残留，同样会被替换成非法形式）。
    // lines() 会丢弃末尾换行，用 join 重建；若原始 src 末尾有换行则补回。
    let mut cleaned_src = cleaned_lines.join("\n");
    if src.ends_with('\n') && !cleaned_src.ends_with('\n') {
        cleaned_src.push('\n');
    }

    // Bug 1 修复：在 cleaned_src（不含 UBO 声明）上先替换变量引用，
    // 这样后续插入的 UBO 声明块内部不会被替换污染。
    let mut result = cleaned_src;
    for (_, name) in &uniforms {
        // 使用词边界，只替换标识符引用（不替换类型名等）
        let name_re = Regex::new(&format!(r"\b{}\b", regex::escape(name))).unwrap();
        // 注意：可能误改同名字符串/局部变量，但 MC 着色器中极少出现
        result = name_re
            .replace_all(&result, &format!("UniformBlock.{}", name))
            .into_owned();
    }

    // 确定 binding（在替换后的源码上查找，避免与已有 UBO/SSBO/sampler binding 冲突）
    let binding = find_available_binding(&result);

    // 构建 UBO 声明（成员名用原始 name，不做替换）
    let mut ubo_decl = format!(
        "layout(std140, binding = {}) uniform UniformBlock {{\n",
        binding
    );
    for (ty, name) in &uniforms {
        ubo_decl.push_str(&format!("    {} {};\n", ty, name));
    }
    ubo_decl.push_str("};\n");

    // 插入 UBO 声明到 #version 行之后
    let insert_pos = find_insert_position(&result);
    let mut final_result = String::with_capacity(result.len() + ubo_decl.len());
    final_result.push_str(&result[..insert_pos]);
    final_result.push_str(&ubo_decl);
    final_result.push_str(&result[insert_pos..]);

    final_result
}

/// 扫描源中所有已使用的 binding 编号（包括 UBO、SSBO、sampler 等），
/// 返回最小未使用的编号（从 0 开始递增直到找到未使用的）。
/// 用于给新注入的 UBO/SSBO 分配不冲突的 binding。
fn find_available_binding(src: &str) -> u32 {
    let binding_re = Regex::new(r"binding\s*=\s*(\d+)").unwrap();
    let mut used = HashSet::new();
    for caps in binding_re.captures_iter(src) {
        if let Some(m) = caps.get(1) {
            if let Ok(val) = m.as_str().parse::<u32>() {
                used.insert(val);
            }
        }
    }
    // 从 0 开始找最小未使用编号（但通常递增分配更简单）
    let mut candidate = 0;
    while used.contains(&candidate) {
        candidate += 1;
    }
    candidate
}

/// 返回适合插入新声明的位置（在 #version 行之后，若有则在其后，否则在开头）
fn find_insert_position(src: &str) -> usize {
    if let Some(version_line) = src.lines().find(|l| l.trim_start().starts_with("#version")) {
        // 找到该行结束位置（包括换行符）
        let line_end = version_line.as_ptr() as usize + version_line.len() - src.as_ptr() as usize;
        // 加上换行符偏移（如果有）
        let pos = line_end
            + if src[line_end..].starts_with('\n') {
                1
            } else {
                0
            };
        pos
    } else {
        0
    }
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
        // 桌面 GLSL < 460 统一升级到 450 core（支持 layout 限定符）
        let mut result = "#version 120\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 450 core"));
    }

    #[test]
    fn test_force_glsl_version_keep_150() {
        // #version 150 升级到 450 core（layout 限定符需要 420+）
        let mut result = "#version 150\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 450 core"));
    }

    #[test]
    fn test_force_glsl_version_keep_330() {
        // #version 330 升级到 450 core（layout 限定符需要 420+）
        let mut result = "#version 330\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 450 core"));
    }

    #[test]
    fn test_force_glsl_version_keep_450() {
        // #version 450 规范化为 core
        let mut result = "#version 450\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 450 core"));
    }

    #[test]
    fn test_force_glsl_version_keep_460() {
        // #version 460 保持版本号，仅规范化为 core
        let mut result = "#version 460\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 460 core"));
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
    fn test_preprocess_full_pipeline() {
        let input = "#version 330\nlayout(std140) uniform MyBlock {\n    mat4 data;\n};\nin vec4 color;\nout vec4 fragColor;\nuniform mat4 MVP;\nvoid main() {\n    fragColor = color;\n}\n";
        let result = preprocess(input);
        // 版本升级到 450 core（layout 限定符需要 420+）
        assert!(result.contains("#version 450 core"));
        // UBO 应有 binding。注意：独立 uniform MVP 被 convert_uniforms_to_ubo 包装成
        // UniformBlock（binding=0），原生 MyBlock 被 inject_missing_bindings 分配 binding=1。
        // 只验证 MyBlock 有 binding，不关心具体值（避免 binding 编号分配顺序耦合）。
        assert!(
            result.contains("layout(std140, binding=") && result.contains("uniform MyBlock"),
            "MyBlock 应有 binding，实际: {}",
            result
        );
        // in/out 应有 location，且 in/out 独立计数（都从 0 开始）
        assert!(result.contains("layout(location=0) in vec4 color;"));
        assert!(result.contains("layout(location=0) out vec4 fragColor;"));
    }

    /// 验证移除 MC moj_import 产生的 /*#version N*/ 注释
    /// 复现 1.21.11 场景：fragment shader 源码含 `/*#version 330*/` 注释
    #[test]
    fn test_strip_mc_version_comment_in_preprocess() {
        let input = "#version 330\n#line 0 1\n/*#version 330*/\nvoid main() {}\n";
        let result = preprocess(input);
        // /*#version 330*/ 应被移除
        assert!(
            !result.contains("/*#version"),
            "/*#version N*/ 注释应被移除，实际: {}",
            result
        );
        // #line 也应被移除
        assert!(!result.contains("#line"));
        // 版本应升级到 450 core
        assert!(result.contains("#version 450 core"));
    }

    /// preprocess 将 #version 150 升级到 450 core（layout 限定符需要 420+）
    #[test]
    fn test_preprocess_upgrades_version_150() {
        let input = "#version 150\n\
            uniform mat4 ModelViewMat;\n\
            in vec3 Position;\n\
            out vec4 vertexColor;\n\
            void main() {\n\
                gl_Position = ModelViewMat * vec4(Position, 1.0);\n\
            }\n";
        let result = preprocess(input);
        // 版本升级到 450 core
        assert!(
            result.contains("#version 450 core"),
            "expected #version 450 core, got: {}",
            result
        );
        // in/out 应有 location
        assert!(result.contains("layout(location=0) in vec3 Position;"));
        assert!(result.contains("layout(location=0) out vec4 vertexColor;"));
    }

    /// Bug 1 回归：UBO 声明块内部成员名不应被替换成 UniformBlock.name（带点号非法）
    /// 复现日志中的 "unexpected DOT, expecting COMMA or SEMICOLON" 错误。
    #[test]
    fn test_convert_uniforms_ubo_member_no_dot() {
        let input = "#version 150\n\
            uniform mat4 ModelViewMat;\n\
            uniform mat4 ProjMat;\n\
            in vec3 Position;\n\
            out vec4 vertexColor;\n\
            void main() {\n\
                gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);\n\
                vertexColor = vec4(1.0);\n\
            }\n";
        let result = preprocess(input);
        // UBO 声明块内部成员名应保持原样（不带点号）
        assert!(
            result.contains("mat4 ModelViewMat;"),
            "UBO 成员名应保持原样，实际: {}",
            result
        );
        assert!(
            result.contains("mat4 ProjMat;"),
            "UBO 成员名应保持原样，实际: {}",
            result
        );
        // 不应出现带点号的成员声明（Bug 1 的症状）
        assert!(
            !result.contains("mat4 UniformBlock."),
            "UBO 成员名不应带点号（Bug 1），实际: {}",
            result
        );
    }

    /// Bug 2 回归：原始 uniform 声明行应被删除，不应残留（残留行也会被替换成非法形式）
    #[test]
    fn test_convert_uniforms_original_line_removed() {
        let input = "#version 150\n\
            uniform mat4 ModelViewMat;\n\
            in vec3 Position;\n\
            void main() {\n\
                gl_Position = ModelViewMat * vec4(Position, 1.0);\n\
            }\n";
        let result = preprocess(input);
        // 原始独立 uniform 声明行应被删除（不残留 `uniform mat4 ModelViewMat;`）
        // UBO 块内的成员声明不是 `uniform mat4`，是 `mat4 ModelViewMat;`
        assert!(
            !result.contains("uniform mat4 ModelViewMat;"),
            "原始 uniform 声明行应被删除（Bug 2），实际: {}",
            result
        );
        // 但引用应被替换为 UniformBlock.ModelViewMat
        assert!(
            result.contains("UniformBlock.ModelViewMat"),
            "引用应替换为 UniformBlock.name，实际: {}",
            result
        );
    }

    /// Bug 3 回归：独立 uniform UBO 与原生 UBO 的 binding 不应冲突
    /// 场景：shader 同时有独立 uniform 和原生 layout(std140) UBO，
    /// convert_uniforms_to_ubo 给 UniformBlock 分配 binding=0，
    /// inject_missing_bindings 给原生 UBO 也应分配不冲突的 binding。
    #[test]
    fn test_ubo_binding_no_conflict_with_native_ubo() {
        let input = "#version 330\n\
            layout(std140) uniform DynamicTransforms {\n\
                mat4 ModelViewMat;\n\
            };\n\
            uniform vec4 ColorModulator;\n\
            in vec3 Position;\n\
            out vec4 vertexColor;\n\
            void main() {\n\
                gl_Position = ModelViewMat * vec4(Position, 1.0);\n\
                vertexColor = ColorModulator;\n\
            }\n";
        let result = preprocess(input);
        // 两个 UBO 应有不同 binding
        // UniformBlock（独立 uniform 包装）和 DynamicTransforms（原生 UBO）
        // 验证两者 binding 编号不同
        // 注意：binding 写法可能带空格（`binding = 0`，convert_uniforms_to_ubo 生成）
        // 或不带空格（`binding=1`，inject_missing_bindings 生成），用正则统一提取。
        let binding_num_re = regex::Regex::new(r"binding\s*=\s*(\d+)").unwrap();
        let mut nums: Vec<u32> = Vec::new();
        for line in result.lines() {
            if line.contains("uniform") && line.contains("binding") {
                if let Some(caps) = binding_num_re.captures(line) {
                    if let Ok(b) = caps[1].parse::<u32>() {
                        nums.push(b);
                    }
                }
            }
        }
        assert!(
            nums.len() >= 2,
            "应有至少 2 个 UBO（UniformBlock + DynamicTransforms），实际编号: {:?}\n{}",
            nums,
            result
        );
        let unique: std::collections::HashSet<u32> = nums.iter().copied().collect();
        assert_eq!(
            nums.len(),
            unique.len(),
            "UBO binding 编号不应重复（Bug 3），实际编号: {:?}\n{}",
            nums,
            result
        );
    }

    /// 复现日志中 shader 3（vertex）的完整场景：MC 核心 vertex shader
    /// 验证修复后 preprocess 输出合法 GLSL（无点号成员名、uniform 行已删除、引用已替换）
    #[test]
    fn test_mc_core_vertex_shader_preprocess() {
        let input = "#version 150\n\
            in vec3 Position;\n\
            in vec4 Color;\n\
            uniform mat4 ModelViewMat;\n\
            uniform mat4 ProjMat;\n\
            out vec4 vertexColor;\n\
            void main() {\n\
                gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);\n\
                vertexColor = Color;\n\
            }\n";
        let result = preprocess(input);
        // 1. 版本升级
        assert!(result.contains("#version 450 core"));
        // 2. UBO 包装：应有 UniformBlock 声明，成员名不带点号
        assert!(result.contains("uniform UniformBlock"));
        assert!(result.contains("mat4 ModelViewMat;"));
        assert!(result.contains("mat4 ProjMat;"));
        // 3. 原始 uniform 行已删除
        assert!(!result.contains("uniform mat4 ModelViewMat;"));
        assert!(!result.contains("uniform mat4 ProjMat;"));
        // 4. 引用已替换
        assert!(result.contains("UniformBlock.ProjMat * UniformBlock.ModelViewMat"));
        // 5. in/out 有 location
        assert!(result.contains("layout(location=0) in vec3 Position;"));
        assert!(result.contains("layout(location=1) in vec4 Color;"));
        assert!(result.contains("layout(location=0) out vec4 vertexColor;"));
        // 6. 关键：UBO 声明块内部不应有任何点号（Bug 1 的直接症状）
        assert!(
            !result.contains("UniformBlock.UniformBlock."),
            "不应出现嵌套点号引用，实际: {}",
            result
        );
    }

    /// 验证 gl_VertexID → gl_VertexIndex 重命名（Vulkan target 要求）
    #[test]
    fn test_rename_gl_vertex_id() {
        let input = "#version 150\n\
            uniform mat4 ProjMat;\n\
            void main() {\n\
                vec2 uv = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);\n\
                gl_Position = ProjMat * vec4(uv, 0.0, 1.0);\n\
            }\n";
        let result = preprocess(input);
        assert!(
            result.contains("gl_VertexIndex"),
            "gl_VertexID 应重命名为 gl_VertexIndex，实际: {}",
            result
        );
        assert!(
            !result.contains("gl_VertexID"),
            "不应残留 gl_VertexID，实际: {}",
            result
        );
    }

    /// 验证 sampler 作为参数名被重命名（避免与 GLSL 关键字冲突）
    #[test]
    fn test_rename_sampler_param() {
        let input = "#version 330\n\
            uniform sampler2D Tex;\n\
            in vec2 vUV;\n\
            out vec4 fragColor;\n\
            vec4 sampleNearest(sampler2D sampler, vec2 uv) {\n\
                return texture(sampler, uv);\n\
            }\n\
            void main() {\n\
                fragColor = sampleNearest(Tex, vUV);\n\
            }\n";
        let result = preprocess(input);
        // sampler 作为参数名应被重命名为 u_sampler
        assert!(
            result.contains("u_sampler"),
            "sampler 参数名应重命名为 u_sampler，实际: {}",
            result
        );
        // sampler2D 类型名不应被替换（\b 词边界保护）
        assert!(
            result.contains("sampler2D"),
            "sampler2D 类型名应保留，实际: {}",
            result
        );
    }
}
