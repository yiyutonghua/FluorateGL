//! GLSL 源码预处理模块
//!
//! 本分支（glslang-targetgl）使用 OpenGL target 编译 GLSL → SPIR-V。
//! OpenGL target 要求（spike 实测）：
//! - 桌面 GLSL >= 330（#version 150 被拒）→ 统一升级到 450 core
//! - 所有 in/out 有 location → 注入 layout(location)
//! - UBO/SSBO 有 binding → 注入 layout(binding)
//! - non-opaque standalone uniform 需要 location（但无需 UBO 包装）→ 注入
//! - attribute/varying 老语法在 450 core 被移除 → 关键字迁移
//!
//! 保留的注入：
//! - in/out varying 的 layout(location)（OpenGL SPIR-V 要求所有 in/out 有 location）
//! - non-opaque standalone uniform 的 layout(location)（独立计数，与 in/out 空间隔离）
//! - UBO/SSBO 的 layout(binding)
//! - textureQueryLod polyfill（GLES 3.0 不支持 GL 4.0 textureQueryLod，Adreno 老驱动保险）
//!
//! 不再需要（对比 Vulkan target）：
//! - convert_uniforms_to_ubo（standalone uniform 原生合法）
//! - undef_vulkan_macro（OpenGL target 下 glslang 不定义 VULKAN 宏，spike_h 实测）
//! - rename_vulkan_builtin_variables（gl_VertexID 保留原名，spike_e 实测）
//!
//! 重要：in/out varying 的 location 注入使用独立的 in_counter 和 out_counter。
//! in 和 out 是独立的接口空间（SPIR-V Input/Output StorageClass），
//! 分别从 0 计数，保证 VS out 和 FS in 的同名 varying location 一致。

use regex::Regex;
use rustc_hash::FxHashSet;
use std::sync::OnceLock;

/// GLSL stage 常量（与 GL_VERTEX_SHADER/GL_FRAGMENT_SHADER 对齐）
pub const GL_VERTEX_SHADER: u32 = 0x8B31;
pub const GL_FRAGMENT_SHADER: u32 = 0x8B30;

/// GLSL 预处理主入口
///
/// 执行顺序：
/// 1. 移除 #line 指令
/// 2. 移除 MC 的 `/*#version N*\/` 注释（moj_import 文本拼接产物，会干扰 glslang parse）
/// 3. 规范化 GLSL 版本（无版本插入 450，桌面版本统一升级为 450 core）
/// 4. 迁移 attribute/varying 老语法为 in/out（按 stage 区分方向）
/// 5. 为缺少 location 的 in/out 变量自动添加 layout(location=X)（in/out 独立计数）
/// 6. 为缺少 location 的 non-opaque standalone uniform 注入 layout(location=X)
/// 7. 为缺少 binding 的 UBO/SSBO 自动添加 layout(binding=X)
///
/// `stage` 参数用于 attribute/varying 关键字迁移：
/// - VS：attribute → in，varying → out
/// - FS：varying → in
pub fn preprocess(source: &str, stage: u32) -> String {
    let mut result = remove_line_directives(source);
    strip_mc_version_comment(&mut result);
    force_glsl_version(&mut result);
    // P4：GL_ARB_derivative_control 宏分支改写（对齐 MobileGlues glsl_for_es.cpp:711-712）。
    // glslang OpenGL target 450 下该扩展宏是否定义取决于实现，若 shader 内有
    // `#ifdef GL_ARB_derivative_control` 分支（如 dFdxFine 优化路径），宏未定义时
    // 走 else 分支——改写为强制走"无该宏"路径（#ifdef→#if 0），与 GLES 实际能力
    // 一致（GLES 无 derivative control 扩展语义，只有 dFdx/dFdy 基础版）。
    replace_all(&mut result, "#ifdef GL_ARB_derivative_control", "#if 0");
    replace_all(&mut result, "#ifndef GL_ARB_derivative_control", "#if 1");
    // attribute/varying 老语法在 450 core 被移除（"removed in version 420"），
    // 按 stage 迁移为 in/out（必须先于 inject_missing_locations，让迁移后的
    // 变量也能获得 location 注入）。
    migrate_legacy_variables(&mut result, stage);
    // samplerBuffer 原样保留（OpenGL target 原生支持 ImageBuffer；转换会破坏 texelFetch 调用）
    // 注入 textureQueryLod polyfill（GLES 3.0 不支持 GL 4.0 textureQueryLod）
    inject_texture_query_lod(&mut result);
    // P2：atomic counter → SSBO 模拟（GLES 驱动 atomic counter 上限小；
    // 桌面 GL 3.3 无 atomic counter（GL 4.2+ 特性），仅 GL 4.x shader 输入时触发）
    let (converted, converted_flag) = convert_atomic_counter_to_ssbo(&result);
    if converted_flag {
        log::debug!(
            "[ShaderTranslator] preprocess: atomic counter 已改写为 SSBO（{} chars -> {} chars）",
            result.len(),
            converted.len()
        );
        result = converted;
    }
    inject_missing_locations(&mut result);
    inject_missing_uniform_locations(&mut result);
    inject_missing_bindings(&mut result);
    result
}

/// P2：atomic counter → SSBO 模拟（对齐 MobileGlues glsl_for_es.cpp:476-559）。
///
/// 背景：GLES 驱动的 atomic counter 上限小（移动 GPU fragment stage 常为 0 或 8，
/// Mesa 为 4096），超出即编译失败；SSBO 的 atomicAdd 无上限且 GLES 3.1+ 原生
/// 支持（已探针验证）。桌面 GL 3.3 无 atomic counter（GL 4.2+），本路径仅在
/// GL 4.x shader 被喂给翻译管线时触发。
///
/// 改写：
/// - 声明 `layout(binding=N, offset=0) uniform atomic_uint NAME;` →
///   `layout(binding=N, std430) buffer AtomicCounterSSBO_N { uint NAME; };`
///   （数组 `atomic_uint NAME[K]` → `uint NAME[K]`，声明块内）
/// - 调用：atomicCounterIncrement(NAME) → atomicAdd(NAME, 1u)、
///   atomicCounterDecrement(NAME) → atomicAdd(NAME, uint(-1))、
///   atomicCounterAdd(NAME, X) → atomicAdd(NAME, X)、
///   atomicCounter(NAME) → NAME（数组索引 `NAME[i]` 均支持）
/// - 文件尾插入 watermark（供运行时/日志识别转换）
///
/// 限制标注：不插入 memoryBarrierBuffer（同一 invocation 内 atomic 顺序由程序
/// 序保证；跨 invocation 可见性由 app 的 glMemoryBarrier 负责——运行时层
/// glMemoryBarrier 已补 GL_SHADER_STORAGE_BARRIER_BIT）。
pub(crate) const ATOMIC_SSBO_WATERMARK: &str = "// [FluorateGL] atomic counter emulated as SSBO";

pub(crate) fn convert_atomic_counter_to_ssbo(source: &str) -> (String, bool) {
    if !source.contains("atomic_uint") && !source.contains("atomicCounter") {
        return (source.to_string(), false);
    }

    let mut result = source.to_string();

    // 1. 声明改写：layout(binding=N[, offset=M]) uniform atomic_uint NAME[K];
    //    → layout(binding=N, std430) buffer AtomicCounterSSBO_N { uint NAME[K]; };
    static RE_DECL: OnceLock<Regex> = OnceLock::new();
    let re_decl = RE_DECL.get_or_init(|| {
        Regex::new(
            r"(?i)layout\s*\(\s*binding\s*=\s*(\d+)\s*(?:,\s*offset\s*=\s*\d+\s*)?\)\s*uniform\s+atomic_uint\s+(\w+)(\s*\[\s*\d+\s*\])?\s*;",
        )
        .unwrap()
    });
    let mut decl_found = false;
    result = re_decl
        .replace_all(&result, |caps: &regex::Captures| {
            decl_found = true;
            let binding = &caps[1];
            let name = &caps[2];
            let arr = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            format!(
                "layout(binding={}, std430) buffer AtomicCounterSSBO_{} {{ uint {}{}; }};",
                binding, binding, name, arr
            )
        })
        .into_owned();

    // 2. 调用改写（顺序重要：先长模式后短模式，`atomicCounter(` 不会误匹配
    //    `atomicCounterIncrement(`——其后是字母非左括号）
    static RE_INCR: OnceLock<Regex> = OnceLock::new();
    static RE_DECR: OnceLock<Regex> = OnceLock::new();
    static RE_ADD: OnceLock<Regex> = OnceLock::new();
    static RE_READ: OnceLock<Regex> = OnceLock::new();
    let re_incr = RE_INCR.get_or_init(|| {
        Regex::new(r"(?i)\batomicCounterIncrement\s*\(\s*(\w+)(\s*\[[^\]]*\])?\s*\)").unwrap()
    });
    let re_decr = RE_DECR.get_or_init(|| {
        Regex::new(r"(?i)\batomicCounterDecrement\s*\(\s*(\w+)(\s*\[[^\]]*\])?\s*\)").unwrap()
    });
    let re_add = RE_ADD.get_or_init(|| {
        Regex::new(r"(?i)\batomicCounterAdd\s*\(\s*(\w+)(\s*\[[^\]]*\])?\s*,\s*([^)]+)\)").unwrap()
    });
    let re_read = RE_READ.get_or_init(|| {
        Regex::new(r"(?i)\batomicCounter\s*\(\s*(\w+)(\s*\[[^\]]*\])?\s*\)").unwrap()
    });
    let mut call_found = false;
    let mut has_call = |r: &Regex, s: &str, repl: &str| -> String {
        let out = r
            .replace_all(s, |caps: &regex::Captures| {
                call_found = true;
                let name = &caps[1];
                let idx = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                match repl {
                    "incr" => format!("atomicAdd({}{}, 1u)", name, idx),
                    "decr" => format!("atomicAdd({}{}, uint(-1))", name, idx),
                    "add" => format!("atomicAdd({}{}, {})", name, idx, &caps[3]),
                    "read" => format!("{}{}", name, idx),
                    _ => unreachable!(),
                }
            })
            .into_owned();
        out
    };
    // 顺序：Increment/Decrement/Add 先（更长模式），Counter 读取最后
    result = has_call(&re_incr, &result, "incr");
    result = has_call(&re_decr, &result, "decr");
    result = has_call(&re_add, &result, "add");
    result = has_call(&re_read, &result, "read");

    let converted = decl_found || call_found;
    if converted {
        result.push('\n');
        result.push_str(ATOMIC_SSBO_WATERMARK);
        result.push('\n');
    }
    (result, converted)
}

/// 字符串级全局替换（与 MobileGlues replace_all 对齐）。
fn replace_all(result: &mut String, from: &str, to: &str) {
    if from.is_empty() {
        return;
    }
    let mut start = 0;
    while let Some(pos) = result[start..].find(from) {
        let abs = start + pos;
        result.replace_range(abs..abs + from.len(), to);
        start = abs + to.len();
    }
}

/// 迁移 GLSL 老语法的 attribute/varying 关键字为 in/out
///
/// 450 core profile 已移除 attribute/varying（"removed in version 420"），
/// preprocess 统一升级到 450 后老语法编译必然失败（spike_f 实测）。
/// 按 stage 迁移：
/// - VS：attribute → in，varying → out
/// - FS：varying → in
///
/// attribute/varying 都是 GLSL 保留关键字，不可能作为标识符出现，
/// 词边界全局替换安全（与 string_pass::replace_legacy_syntax 同策略）。
fn migrate_legacy_variables(result: &mut String, stage: u32) {
    static RE_ATTRIBUTE: OnceLock<Regex> = OnceLock::new();
    static RE_VARYING: OnceLock<Regex> = OnceLock::new();
    let re_attribute = RE_ATTRIBUTE.get_or_init(|| Regex::new(r"\battribute\b").unwrap());
    let re_varying = RE_VARYING.get_or_init(|| Regex::new(r"\bvarying\b").unwrap());

    let mut changed = false;
    match stage {
        GL_VERTEX_SHADER => {
            let s = re_attribute.replace_all(result, "in").into_owned();
            if s != *result {
                changed = true;
                *result = s;
            }
            let s = re_varying.replace_all(result, "out").into_owned();
            if s != *result {
                changed = true;
                *result = s;
            }
        }
        GL_FRAGMENT_SHADER => {
            let s = re_varying.replace_all(result, "in").into_owned();
            if s != *result {
                changed = true;
                *result = s;
            }
        }
        _ => {}
    }
    if changed {
        log::debug!(
            "[ShaderTranslator] preprocess 迁移 attribute/varying → in/out (stage 0x{:04X})",
            stage
        );
    }
}

/// 将 samplerBuffer/isamplerBuffer/usamplerBuffer 转换为对应的 2D sampler（**已禁用**）
///
/// 本函数当前为 no-op（原样返回源码），保留仅为未来 GLES 3.0 设备启用参考。
/// 禁用原因：
/// 1. OpenGL target 原生支持 texture buffer（SPIR-V Capability ImageBuffer 是
///    OpenGL/Vulkan 通用能力），无需转换即可由 shaderc 正常编译。
/// 2. 类型替换会破坏 texelFetch 调用：旧实现只把 `samplerBuffer → sampler2D`
///    类型名替换，而调用改写正则仅匹配 `texelFetch(<类型名>,...)`，实际源码
///    中调用是变量名 `texelFetch(CloudFaces, index)`，正则落空后调用被原样
///    保留 → `isampler2D`（需要 ivec2 坐标）+ int 坐标 → glslang 报重载失败
///    → 触发 string_pass 回退 → 崩溃。
/// 3. 旧方案注入的 `u_BufferTexWidth` uniform 需额外设置（采样位置错误）。
///
/// 补充：GLES 3.2 core 已含 texture buffer 功能（GL_EXT_texture_buffer 扩展
/// 在 ES 300/310 下由 spirv-cross 自动声明），转换并非 GLES 侧硬性需求。
///
/// 若未来需要在 GLES 3.0 设备上启用（折行模拟 buffer 纹理），应重写调用改写
/// 逻辑为按变量名匹配，并处理 u_BufferTexWidth 的 uniform 提供方式。
#[allow(dead_code)]
fn convert_sampler_buffer(src: &str) -> String {
    src.to_string()
}

/// 注入 textureQueryLod polyfill 函数并替换调用点
///
/// GLES 3.0 不支持 textureQueryLod（GL 4.0），用 dFdx/dFdy + log2 软件实现。
/// 参考 MobileGlues 的 inject_textureQueryLod。
///
/// 实现：
/// 1. 在 #version 行之后注入两个 polyfill 重载（sampler2D / sampler3D）
/// 2. 用正则替换调用点 `textureQueryLod(` → `textureQueryLod_polyfill(`
///
/// 注意：只替换函数调用，polyfill 函数自身定义（`textureQueryLod_polyfill(`）
/// 不会被误匹配（`textureQueryLod` 后是 `_`，不满足 `\(`）。
/// 若 shader 不含 textureQueryLod 则直接返回，无副作用。
fn inject_texture_query_lod(result: &mut String) {
    // 快速检查：不含 textureQueryLod 直接返回
    if !result.contains("textureQueryLod") {
        return;
    }

    // 注入 polyfill 函数（在 #version 行之后）
    let polyfill = "\
// textureQueryLod polyfill (GLES 3.0 不支持 GL 4.0 textureQueryLod)
vec2 textureQueryLod_polyfill(sampler2D sampler, vec2 coords) {
  vec2 dx = dFdx(coords);
  vec2 dy = dFdy(coords);
  float maxDelta = max(dot(dx, dx), dot(dy, dy));
  float lod = 0.5 * log2(maxDelta);
  return vec2(lod, lod);
}
vec2 textureQueryLod_polyfill(sampler3D sampler, vec3 coords) {
  vec3 dx = dFdx(coords);
  vec3 dy = dFdy(coords);
  float maxDelta = max(dot(dx, dx), dot(dy, dy));
  float lod = 0.5 * log2(maxDelta);
  return vec2(lod, lod);
}
";
    let insert_pos = find_insert_position(result);
    let mut new_result = String::with_capacity(result.len() + polyfill.len());
    new_result.push_str(&result[..insert_pos]);
    new_result.push_str(polyfill);
    new_result.push_str(&result[insert_pos..]);

    // 替换函数调用 textureQueryLod( → textureQueryLod_polyfill(
    // \btextureQueryLod\s*\( 不会匹配 textureQueryLod_polyfill(（d 后是 _，非词边界后的 \s*\()）
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\btextureQueryLod\s*\(").unwrap());
    let replaced = re
        .replace_all(&new_result, "textureQueryLod_polyfill(")
        .into_owned();

    if replaced.len() != new_result.len() {
        log::debug!("[ShaderTranslator] preprocess 注入 textureQueryLod polyfill 并替换调用点");
    }

    *result = replaced;
}

/// 提取 GLSL 源码中的 #version 行
pub fn extract_version(source: &str) -> Option<&str> {
    source
        .lines()
        .find(|l| l.trim_start().starts_with("#version"))
}

/// 移除 #line 指令（对齐 MobileGlues replace_line_starting_with("#line")）
fn remove_line_directives(source: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?m)^\s*#line\s+.*$(\n|$)?").unwrap());
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
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"/\*#version\s+\d+\s*\*/").unwrap());
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

/// 规范化 GLSL 版本以满足 OpenGL target + layout 限定符要求
///
/// OpenGL SPIR-V 要求桌面 GLSL >= 330，且 preprocess 注入的
/// `layout(binding=)` 需要 GLSL 420+、`layout(location=)` 对 uniform 需要 430+，
/// 统一升级到 450 core 可全覆盖。
///
/// 策略：
/// - 无 #version → 插入 #version 450 core
/// - 桌面 GLSL 任意版本 → 统一升级/规范化为 450 core（移除 compatibility profile）
/// - ES 版本 → 保持不变
fn force_glsl_version(result: &mut String) {
    let version = extract_version(result);
    match version {
        None => {
            result.insert_str(0, "#version 450 core\n");
        }
        Some(v) => {
            if let Some(_ver) = parse_version_number(v) {
                if is_es_version(v) {
                    // ES 版本保持不变（语法不兼容，升级无意义）
                    return;
                }

                static RE: OnceLock<Regex> = OnceLock::new();
                let re = RE.get_or_init(|| Regex::new(r"(?m)^#version\s+\d+.*$").unwrap());
                // 统一升级到 450 core（支持 layout(binding/location)，移除 compatibility）
                *result = re.replace(result, "#version 450 core").to_string();
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
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"binding\s*=\s*(\d+)").unwrap());
    re.captures(qualifiers)
        .and_then(|c| c[1].parse::<u32>().ok())
}

/// 为缺少 location 的 in/out 变量自动添加 layout(location=X)
///
/// Vulkan target 下 glslang parse 要求所有 in/out 变量必须有 location。
///
/// 关键修复：
/// 1. in 和 out 使用独立的 counter，分别从 0 开始（不同接口空间，不冲突）
/// 2. 增强正则匹配插值修饰符（flat/smooth/noperspective/centroid/patch/invariant），
///    避免漏注入 `flat in vec3 normal;` 等常见声明
/// 3. 已有 layout(location=N) 的声明会推进 counter 到 N+1（而非跳过不推进），
///    避免后续注入的 location 与已有值冲突
/// 4. 数组声明 `in vec4 arr[N];` 按 N 推进 counter（占 N 个 location）
/// 5. 增强正则的健壮性，处理更多边缘情况（如多行声明、注释中的匹配）
fn inject_missing_locations(result: &mut String) {
    // 匹配带可选前导 layout 和插值修饰符的 in/out 声明
    // 情况1: [layout(...)] [修饰符] in/out type name[;];
    // 情况2: [修饰符] in/out type name;
    // 增强版正则：更精确匹配，避免误匹配注释或字符串中的内容
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<indent>\s*)(?:layout\s*\(\s*(?P<layout_qual>[^)]*)\s*\)\s*)?(?P<prefix>(?:(?:flat|smooth|noperspective|centroid|patch|precise|invariant)\s+)*)(?P<qualifier>in|out)\s+(?P<rest>.+?;[^\n]*)$"
        ).unwrap()
    });

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
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"location\s*=\s*(\d+)").unwrap());
    re.captures(layout_qual)
        .and_then(|c| c[1].parse::<u32>().ok())
}

/// 解析变量声明中的数组大小（用于计算 location 占用数）
/// 输入如 "vec4 colors[4];" → 返回 4，"vec4 color;" → 返回 1
fn parse_array_size(rest: &str) -> u32 {
    let Some(bracket_start) = rest.find('[') else {
        return 1;
    };
    let Some(bracket_end_rel) = rest[bracket_start..].find(']') else {
        return 1;
    };
    let size_str = &rest[bracket_start + 1..bracket_start + bracket_end_rel];
    match size_str.trim().parse::<u32>() {
        Ok(size) => size.max(1),
        Err(_) => 1,
    }
}

/// 为缺少 binding 的 UBO/SSBO 自动添加 layout(binding=X)
///
/// OpenGL SPIR-V 要求所有 uniform buffer 和 shader storage buffer 必须有 binding
/// （spike_b 实测：无 binding 报 "uniform/buffer blocks require layout(binding=X)"）。
/// 桌面 GLSL 允许省略 binding（由链接器分配），但 SPIR-V 不允许。
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
    static RE_LAYOUT_BLOCK: OnceLock<Regex> = OnceLock::new();
    let re_layout_block = RE_LAYOUT_BLOCK.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<indent>\s*)layout\s*\(\s*(?P<qualifiers>[^)]*)\s*\)\s*(?P<kind>uniform|buffer)\s+(?P<name>\w+)\s*\{",
        )
        .unwrap()
    });

    // 匹配无 layout 的 uniform/buffer 块声明
    // 例如：
    //   uniform MyBlock {
    static RE_PLAIN_BLOCK: OnceLock<Regex> = OnceLock::new();
    let re_plain_block = RE_PLAIN_BLOCK.get_or_init(|| {
        Regex::new(r"(?m)^(?P<indent>\s*)(?P<kind>uniform|buffer)\s+(?P<name>\w+)\s*\{").unwrap()
    });

    // 初始 binding_counter 从已有 binding 的最大值+1 开始，避免与
    // 已有带 binding 的块冲突（sampler 的 binding 由 glslang 自动分配，
    // 此处 find_available_binding 扫描所有已有 binding，从 0 找第一个空闲）。
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

/// 为缺少 location 的 non-opaque standalone uniform 注入 layout(location=N)
///
/// OpenGL SPIR-V 要求 non-opaque standalone uniform 必须有 location
/// （spike_0 实测："non-opaque uniform variables need a layout(location=L)"）。
/// 与 Vulkan target 不同，standalone uniform 原生合法，无需包装进 UBO。
///
/// 计数规则：
/// - 独立 counter（从 0 开始），与 in/out 的 location 空间互不冲突
///   （spike_a 实测：`layout(location=0) uniform mat4` 与 `layout(location=0) in`
///   并存合法）
/// - 只处理单行声明 `uniform T name;`（可选 layout(...) 前缀、可选精度限定符）
/// - 跳过 opaque 类型（sampler/image/atomic_uint——sampler 不需要 location，
///   binding 由 glslang 自动分配）
/// - 跳过 block 声明（`uniform Name {` 无分号结尾，天然不匹配；单行 block
///   `uniform Block { mat4 m; };` 因 `\s*;` 紧跟在 name 之后的要求而不匹配）
/// - 已有 location 的声明保留原值，并推进 counter 到 location+1（避免冲突）
/// - 多行声明（如 `uniform mat4\nModelViewMat;`）不注入：无法安全改写行首，
///   此类 shader 走 string_pass 兜底
fn inject_missing_uniform_locations(result: &mut String) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<indent>\s*)(?:layout\s*\(\s*(?P<layout_qual>[^)]*)\s*\)\s*)?uniform\s+(?P<type>(?:highp\s+|mediump\s+|lowp\s+)?[a-zA-Z_][\w]*)\s+(?P<name>[a-zA-Z_][\w]*)\s*;"
        )
        .unwrap()
    });

    let mut counter: u32 = 0;
    let mut injected = false;
    let mut modified = String::with_capacity(result.len());

    for line in result.lines() {
        if let Some(caps) = re.captures(line) {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let layout_qual = caps.name("layout_qual").map(|m| m.as_str()).unwrap_or("");
            let ty = caps.name("type").map(|m| m.as_str()).unwrap_or("");
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or("");

            // 跳过 opaque 类型（sampler/image/atomic_uint）：不需要 location
            if ty.contains("sampler") || ty.contains("image") || ty.contains("atomic_uint") {
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 已有 location：保留原值，推进 counter 避免后续注入冲突
            if layout_qual.contains("location") {
                if let Some(loc) = parse_layout_location(layout_qual) {
                    counter = counter.max(loc + 1);
                }
                modified.push_str(line);
                modified.push('\n');
                continue;
            }

            // 注入 layout(location=N)。已有非 location 限定符（如 column_major）
            // 保留并追加 location，避免丢失矩阵布局信息。
            let new_line = if layout_qual.trim().is_empty() {
                format!(
                    "{}layout(location={}) uniform {} {};",
                    indent, counter, ty, name
                )
            } else {
                format!(
                    "{}layout({}, location={}) uniform {} {};",
                    indent,
                    layout_qual.trim(),
                    counter,
                    ty,
                    name
                )
            };
            modified.push_str(&new_line);
            modified.push('\n');
            counter += 1;
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

    if injected {
        log::debug!("[ShaderTranslator] preprocess 注入了 standalone uniform location");
    }

    *result = modified;
}

/// 扫描源中所有已使用的 binding 编号（包括 UBO、SSBO、sampler 等），
/// 返回最小未使用的编号（从 0 开始递增直到找到未使用的）。
/// 用于给新注入的 UBO/SSBO 分配不冲突的 binding。
fn find_available_binding(src: &str) -> u32 {
    static BINDING_RE: OnceLock<Regex> = OnceLock::new();
    let binding_re = BINDING_RE.get_or_init(|| Regex::new(r"binding\s*=\s*(\d+)").unwrap());
    let mut used = FxHashSet::default();
    for caps in binding_re.captures_iter(src) {
        if let Some(val) = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) {
            used.insert(val);
        }
    }
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
        line_end
            + if src[line_end..].starts_with('\n') {
                1
            } else {
                0
            }
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

    /// P4：GL_ARB_derivative_control 宏分支应被改写为强制走无该宏路径
    #[test]
    fn test_derivative_control_macro_rewrite() {
        let input = "#version 330 core\n#ifdef GL_ARB_derivative_control\nfloat a = dFdxFine(x);\n#else\nfloat a = dFdx(x);\n#endif\n#ifndef GL_ARB_derivative_control\nfloat b = dFdy(x);\n#endif\nvoid main() {}\n";
        let result = preprocess(input, 0x8B31);
        assert!(
            result.contains("#if 0\nfloat a = dFdxFine(x);"),
            "#ifdef 应改写为 #if 0（强制跳过扩展分支），got: {}",
            result
        );
        assert!(
            result.contains("#if 1\nfloat b = dFdy(x);"),
            "#ifndef 应改写为 #if 1（强制进入无扩展分支），got: {}",
            result
        );
        assert!(
            !result.contains("GL_ARB_derivative_control"),
            "改写后不应残留扩展宏名"
        );
    }

    /// P2：atomic counter 声明与调用应改写为 SSBO + atomicAdd
    #[test]
    fn test_atomic_counter_to_ssbo_conversion() {
        let input = "#version 450 core\n\
layout(binding = 2, offset = 0) uniform atomic_uint counter;\n\
layout(binding = 3, offset = 4) uniform atomic_uint counters[4];\n\
void main() {\n\
    uint a = atomicCounterIncrement(counter);\n\
    atomicCounterDecrement(counters[1]);\n\
    atomicCounterAdd(counter, 5u);\n\
    uint b = atomicCounter(counters[2]);\n\
}\n";
        let (out, converted) = convert_atomic_counter_to_ssbo(input);
        assert!(converted, "应检测到 atomic counter 并转换");
        assert!(
            out.contains("layout(binding=2, std430) buffer AtomicCounterSSBO_2 { uint counter; };"),
            "声明应改写为 SSBO，got:\n{}",
            out
        );
        assert!(
            out.contains("layout(binding=3, std430) buffer AtomicCounterSSBO_3 { uint counters[4]; };"),
            "数组声明应改写为 SSBO 数组，got:\n{}",
            out
        );
        assert!(
            out.contains("atomicAdd(counter, 1u)"),
            "Increment 应改写为 atomicAdd(+1)，got:\n{}",
            out
        );
        assert!(
            out.contains("atomicAdd(counters[1], uint(-1))"),
            "Decrement 应改写为 atomicAdd(-1)，got:\n{}",
            out
        );
        assert!(
            out.contains("atomicAdd(counter, 5u)"),
            "Add 应改写为 atomicAdd(+X)，got:\n{}",
            out
        );
        assert!(
            out.contains("uint b = counters[2];"),
            "Counter 读取应改为直接变量访问，got:\n{}",
            out
        );
        assert!(
            out.contains(ATOMIC_SSBO_WATERMARK),
            "应插入 watermark"
        );
        assert!(
            !out.contains("atomic_uint"),
            "不应残留 atomic_uint 声明，got:\n{}",
            out
        );
    }

    /// P2：无 atomic 的 shader 应原样返回（不转换不加水印）
    #[test]
    fn test_atomic_counter_to_ssbo_noop() {
        let input = "#version 330 core\nvoid main() {}\n";
        let (out, converted) = convert_atomic_counter_to_ssbo(input);
        assert!(!converted, "无 atomic counter 不应转换");
        assert_eq!(out, input, "应原样返回");
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
        // #version 460 也统一升级到 450 core（OpenGL target 统一 450 策略）
        let mut result = "#version 460\nvoid main() {}".to_string();
        force_glsl_version(&mut result);
        assert!(result.starts_with("#version 450 core"));
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
        let result = preprocess(input, 0x8B31);
        // 版本升级到 450 core（layout 限定符需要 420+）
        assert!(result.contains("#version 450 core"));
        // UBO 应有 binding（无 UniformBlock 包装占位，MyBlock 从 binding=0 开始）
        assert!(
            result.contains("layout(std140, binding=0) uniform MyBlock"),
            "MyBlock 应有 binding=0，实际: {}",
            result
        );
        // in/out 应有 location，且 in/out 独立计数（都从 0 开始）
        assert!(result.contains("layout(location=0) in vec4 color;"));
        assert!(result.contains("layout(location=0) out vec4 fragColor;"));
        // standalone uniform 保留原样并注入 location（独立计数空间，从 0 开始）
        assert!(
            result.contains("layout(location=0) uniform mat4 MVP;"),
            "MVP 应为 standalone uniform 且带 location，实际: {}",
            result
        );
    }

    /// 验证移除 MC moj_import 产生的 /*#version N*/ 注释
    /// 复现 1.21.11 场景：fragment shader 源码含 `/*#version 330*/` 注释
    #[test]
    fn test_strip_mc_version_comment_in_preprocess() {
        let input = "#version 330\n#line 0 1\n/*#version 330*/\nvoid main() {}\n";
        let result = preprocess(input, 0x8B31);
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
        let result = preprocess(input, 0x8B31);
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

    /// 验证 non-opaque standalone uniform 被注入 location（OpenGL target 要求）
    /// 且保留为 standalone 声明（不再包装进 UBO）
    #[test]
    fn test_inject_uniform_locations_basic() {
        let input = "#version 450 core\n\
            uniform mat4 ModelViewMat;\n\
            uniform vec4 ColorModulator;\n\
            uniform float FogStart;\n\
            void main() {}\n";
        let mut result = input.to_string();
        inject_missing_uniform_locations(&mut result);
        assert!(
            result.contains("layout(location=0) uniform mat4 ModelViewMat;"),
            "got: {}",
            result
        );
        assert!(
            result.contains("layout(location=1) uniform vec4 ColorModulator;"),
            "got: {}",
            result
        );
        assert!(
            result.contains("layout(location=2) uniform float FogStart;"),
            "got: {}",
            result
        );
        // standalone 声明保留（无 UBO 包装）
        assert!(
            result.contains("uniform mat4 ModelViewMat"),
            "got: {}",
            result
        );
        assert!(!result.contains("UniformBlock"), "got: {}", result);
    }

    /// 验证 opaque（sampler/image/atomic_uint）不注入 location，且不占用计数
    #[test]
    fn test_inject_uniform_locations_skips_opaque() {
        let input = "#version 450 core\n\
            uniform sampler2D Tex;\n\
            uniform mat4 MVP;\n\
            uniform image2D img;\n\
            uniform atomic_uint counter;\n\
            uniform vec3 scale;\n\
            void main() {}\n";
        let mut result = input.to_string();
        inject_missing_uniform_locations(&mut result);
        assert!(
            result.contains("uniform sampler2D Tex;"),
            "sampler 不应有 location，got: {}",
            result
        );
        assert!(
            result.contains("uniform image2D img;"),
            "image 不应有 location，got: {}",
            result
        );
        assert!(
            result.contains("uniform atomic_uint counter;"),
            "atomic_uint 不应有 location，got: {}",
            result
        );
        // sampler/image/atomic 不占用计数：MVP=0、scale=1
        assert!(
            result.contains("layout(location=0) uniform mat4 MVP;"),
            "got: {}",
            result
        );
        assert!(
            result.contains("layout(location=1) uniform vec3 scale;"),
            "got: {}",
            result
        );
    }

    /// 验证已有 location 的 uniform 保留原值并推进 counter
    #[test]
    fn test_inject_uniform_locations_skips_existing() {
        let input = "#version 450 core\n\
            layout(location=5) uniform mat4 MVP;\n\
            uniform vec4 color;\n\
            void main() {}\n";
        let mut result = input.to_string();
        inject_missing_uniform_locations(&mut result);
        assert!(
            result.contains("layout(location=5) uniform mat4 MVP;"),
            "已有 location 应保留，got: {}",
            result
        );
        // counter 推进到 6，避免与已有 location=5 冲突
        assert!(
            result.contains("layout(location=6) uniform vec4 color;"),
            "后续 uniform 应从 location=6 开始，got: {}",
            result
        );
    }

    /// 验证 uniform location 与 in/out location 使用独立计数空间
    /// （spike_a 实测：两者可共存且都从 0 开始）
    #[test]
    fn test_inject_uniform_locations_independent_from_varying() {
        let input = "#version 450 core\n\
            uniform mat4 MVP;\n\
            in vec3 Position;\n\
            out vec4 fragColor;\n\
            void main() {}\n";
        let result = preprocess(input, 0x8B31);
        assert!(
            result.contains("layout(location=0) uniform mat4 MVP;"),
            "uniform location 独立空间从 0 开始，got: {}",
            result
        );
        assert!(
            result.contains("layout(location=0) in vec3 Position;"),
            "in location 独立空间从 0 开始，got: {}",
            result
        );
        assert!(
            result.contains("layout(location=0) out vec4 fragColor;"),
            "out location 独立空间从 0 开始，got: {}",
            result
        );
    }

    /// 验证 VS 的 attribute/varying 老语法迁移为 in/out
    #[test]
    fn test_migrate_legacy_variables_vertex() {
        let input = "#version 150\n\
            attribute vec3 Position;\n\
            attribute vec4 Color;\n\
            varying vec4 vertexColor;\n\
            void main() {\n\
                gl_Position = vec4(Position, 1.0);\n\
                vertexColor = Color;\n\
            }\n";
        let result = preprocess(input, 0x8B31);
        assert!(
            !result.contains("attribute"),
            "attribute 应迁移为 in，got: {}",
            result
        );
        assert!(
            !result.contains("varying"),
            "varying 应迁移为 out，got: {}",
            result
        );
        // 迁移后仍应注入 location（in/out 独立计数）
        assert!(
            result.contains("layout(location=0) in vec3 Position;"),
            "got: {}",
            result
        );
        assert!(
            result.contains("layout(location=1) in vec4 Color;"),
            "got: {}",
            result
        );
        assert!(
            result.contains("layout(location=0) out vec4 vertexColor;"),
            "got: {}",
            result
        );
    }

    /// 验证 FS 的 varying 迁移为 in（VS 的 attribute 不处理）
    #[test]
    fn test_migrate_legacy_variables_fragment() {
        let input = "#version 150\n\
            varying vec4 vertexColor;\n\
            void main() {\n\
                gl_FragColor = vertexColor;\n\
            }\n";
        let result = preprocess(input, 0x8B30);
        assert!(
            result.contains("layout(location=0) in vec4 vertexColor;"),
            "FS 的 varying 应迁移为 in 并注入 location，got: {}",
            result
        );
        assert!(!result.contains("varying"), "got: {}", result);
    }

    /// 验证 attribute/varying 迁移不误伤 in/out 新语法和 sampler2D 类型
    #[test]
    fn test_migrate_legacy_variables_no_collateral() {
        let input = "#version 330\n\
            in vec3 Position;\n\
            out vec4 vertexColor;\n\
            uniform sampler2D Sampler0;\n\
            void main() {}\n";
        let result = preprocess(input, 0x8B31);
        assert!(result.contains("in vec3 Position;"), "got: {}", result);
        assert!(result.contains("out vec4 vertexColor;"), "got: {}", result);
        assert!(result.contains("sampler2D"), "got: {}", result);
    }

    /// 回归：standalone uniform 与原生 UBO 共存（standalone 不再包装 UBO）
    /// 原生 UBO 的 binding 从 0 开始（无 UniformBlock 占位），
    /// standalone uniform 带 location 且与 in/out 空间独立
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
        let result = preprocess(input, 0x8B31);
        // 原生 UBO 从 binding=0 开始（无 UniformBlock 包装占位）
        assert!(
            result.contains("layout(std140, binding=0) uniform DynamicTransforms"),
            "got: {}",
            result
        );
        // standalone uniform 保留并注入 location（不占用 binding 空间）
        assert!(
            result.contains("layout(location=0) uniform vec4 ColorModulator;"),
            "got: {}",
            result
        );
        // 不应有 UniformBlock 包装
        assert!(!result.contains("UniformBlock"), "got: {}", result);
    }

    /// 复现日志中 shader 3（vertex）的完整场景：MC 核心 vertex shader
    /// 验证修复后 preprocess 输出合法 GLSL（standalone uniform 保留 + location 注入）
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
        let result = preprocess(input, 0x8B31);
        // 1. 版本升级
        assert!(result.contains("#version 450 core"));
        // 2. standalone uniform 保留原样（无 UBO 包装），并注入独立计数的 location
        assert!(result.contains("uniform mat4 ModelViewMat;"));
        assert!(result.contains("uniform mat4 ProjMat;"));
        assert!(result.contains("layout(location=0) uniform mat4 ModelViewMat;"));
        assert!(result.contains("layout(location=1) uniform mat4 ProjMat;"));
        assert!(
            !result.contains("UniformBlock"),
            "不应有 UBO 包装，实际: {}",
            result
        );
        // 3. 引用保持原样
        assert!(
            result.contains("gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);"),
            "引用应保持原样，实际: {}",
            result
        );
        // 4. in/out 有 location
        assert!(result.contains("layout(location=0) in vec3 Position;"));
        assert!(result.contains("layout(location=1) in vec4 Color;"));
        assert!(result.contains("layout(location=0) out vec4 vertexColor;"));
    }

    /// 验证 gl_VertexID 保留原名（OpenGL target 语义，spike_e 实测）
    #[test]
    fn test_gl_vertex_id_preserved() {
        let input = "#version 150\n\
            uniform mat4 ProjMat;\n\
            void main() {\n\
                vec2 uv = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);\n\
                gl_Position = ProjMat * vec4(uv, 0.0, 1.0);\n\
            }\n";
        let result = preprocess(input, 0x8B31);
        assert!(
            result.contains("gl_VertexID"),
            "gl_VertexID 应保留原名，实际: {}",
            result
        );
        assert!(
            !result.contains("gl_VertexIndex"),
            "不应出现 Vulkan 名 gl_VertexIndex，实际: {}",
            result
        );
    }

    /// 验证 sampler 作为参数名不再被重命名（OpenGL target 无关键字冲突 hack）
    #[test]
    fn test_sampler_param_preserved() {
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
        let result = preprocess(input, 0x8B31);
        // sampler 参数名应保留原样（无 u_sampler 重命名）
        assert!(
            result.contains("sampler2D sampler, vec2 uv"),
            "sampler 参数名应保留，实际: {}",
            result
        );
        assert!(
            !result.contains("u_sampler"),
            "不应出现 u_sampler 重命名产物，实际: {}",
            result
        );
        // sampler2D 类型名应保留
        assert!(result.contains("sampler2D"), "实际: {}", result);
    }
}
