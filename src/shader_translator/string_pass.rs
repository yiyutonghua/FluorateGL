#![allow(dead_code)]

use regex::Regex;
use std::sync::OnceLock;

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
#[allow(dead_code)]
const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
#[allow(dead_code)]
const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
#[allow(dead_code)]
const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;

/// A string-based GLSL -> GLSL ES translator.
///
/// This is intentionally simple: it covers the differences between desktop
/// GLSL (as used by Minecraft 1.17+) and GLSL ES 3.0/3.2. It is not a full
/// parser, so very unusual shaders may still fail to compile.
pub fn translate(source: &str, stage: u32) -> String {
    // Already GLSL ES?
    if source.contains("#version 300 es")
        || source.contains("#version 310 es")
        || source.contains("#version 320 es")
    {
        return source.to_string();
    }

    let mut output = source.to_string();

    // 移除 #line 指令（string_pass 不经过 preprocess，需自行处理）
    output = strip_line_directives(&output);

    // 移除 MC 的 /*#version N*/ 注释行（如有）
    output = strip_mc_version_comment(&output);

    // Replace desktop #version directives with GLES equivalents.
    output = replace_version(&output);

    // Ensure a #version line exists.
    if !output.contains("#version ") {
        output.insert_str(0, "#version 300 es\n");
    }

    // Legacy GLSL (<=1.30) uses attribute/varying/texture2D.
    output = replace_legacy_syntax(&output, stage);

    // GLSL ES requires explicit precision for float, int and samplers.
    output = inject_precision(&output, stage);

    // gl_FragColor is not built-in in GLSL ES 300.
    output = replace_frag_color(&output);

    // Some built-in names differ or are unavailable.
    output = replace_builtin_names(&output);

    // 定点修复：MC 光照贴图采样函数中的 ivec2/float 除法
    output = fix_minecraft_lightmap(&output);

    output
}

fn replace_version(source: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?m)^\s*#\s*version\s+(\d+)(?:\s+(core|compatibility|es))?\s*$").unwrap()
    });

    re.replace_all(source, |caps: &regex::Captures| {
        let ver: u32 = caps[1].parse().unwrap_or(0);
        let target = match ver {
            120..=460 => "#version 320 es",
            _ => "#version 300 es",
        };
        target.to_string()
    })
    .into_owned()
}

fn replace_legacy_syntax(source: &str, stage: u32) -> String {
    let mut result = source.to_string();

    // 预构造 attribute/varying 的词匹配正则（静态缓存，避免每次调用重编译）
    static RE_ATTRIBUTE: OnceLock<Regex> = OnceLock::new();
    static RE_VARYING: OnceLock<Regex> = OnceLock::new();
    let re_attribute = RE_ATTRIBUTE.get_or_init(|| word_regex("attribute"));
    let re_varying = RE_VARYING.get_or_init(|| word_regex("varying"));

    // Only legacy GLSL (<=1.30) uses attribute/varying.
    if stage == GL_VERTEX_SHADER {
        result = replace_word(&result, re_attribute, "in");
        result = replace_word(&result, re_varying, "out");
    } else if stage == GL_FRAGMENT_SHADER {
        result = replace_word(&result, re_varying, "in");
    }

    // Legacy texture lookup functions. Order matters: longer names first.
    // 预构造所有 (from, to, Regex) 三元组（静态缓存），消除循环内 18 次重编译开销
    static TEXTURE_REPLACEMENTS: OnceLock<Vec<(&'static str, &'static str, Regex)>> =
        OnceLock::new();
    let replacements = TEXTURE_REPLACEMENTS.get_or_init(|| {
        let pairs: &[(&str, &str)] = &[
            ("texture1DProjLod", "textureLod"),
            ("texture2DProjLod", "textureLod"),
            ("texture3DProjLod", "textureLod"),
            ("texture1DLod", "textureLod"),
            ("texture2DLod", "textureLod"),
            ("texture3DLod", "textureLod"),
            ("textureCubeLod", "textureLod"),
            ("shadow1DProj", "texture"),
            ("shadow2DProj", "texture"),
            ("shadow1D", "texture"),
            ("shadow2D", "texture"),
            ("texture1DProj", "texture"),
            ("texture2DProj", "texture"),
            ("texture3DProj", "texture"),
            ("texture1D", "texture"),
            ("texture2D", "texture"),
            ("texture3D", "texture"),
            ("textureCube", "texture"),
        ];
        pairs
            .iter()
            .map(|&(from, to)| (from, to, word_regex(from)))
            .collect()
    });

    for &(_, to, ref re) in replacements {
        result = replace_word(&result, re, to);
    }

    result
}

fn inject_precision(source: &str, stage: u32) -> String {
    let insert_pos = source.find('\n').map(|p| p + 1).unwrap_or(0);
    let mut decls = String::new();

    if !has_precision_for(source, "float") {
        decls.push_str("precision highp float;\n");
    }
    if !has_precision_for(source, "int") {
        decls.push_str("precision highp int;\n");
    }

    if stage == GL_FRAGMENT_SHADER {
        let samplers: &[&str] = &[
            "sampler2D",
            "sampler3D",
            "samplerCube",
            "isampler2D",
            "usampler2D",
            "sampler2DArray",
            "sampler2DShadow",
            "samplerCubeShadow",
        ];
        for s in samplers {
            if source.contains(s) && !has_precision_for(source, s) {
                decls.push_str(&format!("precision highp {};\n", s));
            }
        }
    }

    if decls.is_empty() {
        return source.to_string();
    }

    let mut result = source[..insert_pos].to_string();
    result.push_str(&decls);
    result.push_str(&source[insert_pos..]);
    result
}

fn has_precision_for(source: &str, ty: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?m)^\s*precision\s+(lowp|mediump|highp)\s+(\w+)\s*;").unwrap()
    });
    re.captures_iter(source)
        .any(|caps| caps.get(2).map(|m| m.as_str() == ty).unwrap_or(false))
}

fn replace_frag_color(source: &str) -> String {
    if !source.contains("gl_FragColor") {
        return source.to_string();
    }

    let mut result = source.replace("gl_FragColor", "fragColor");

    if result.contains("out vec4 fragColor")
        || result.contains("layout(location = 0) out vec4 fragColor")
    {
        return result;
    }

    // If there is a single unlocated out vec4, reuse it as location 0.
    static OUT_RE: OnceLock<Regex> = OnceLock::new();
    let out_re = OUT_RE.get_or_init(|| Regex::new(r"(?m)^\s*out\s+vec4\s+(\w+)\s*;").unwrap());
    let matches: Vec<_> = out_re.find_iter(&result).collect();
    if matches.len() == 1 {
        let name = out_re.captures(matches[0].as_str()).unwrap()[1].to_string();
        result = result.replace(
            &format!("out vec4 {};", name),
            &format!("layout(location = 0) out vec4 {};", name),
        );
        result = result.replace("fragColor", &name);
        return result;
    }

    // Otherwise insert a dedicated location 0 output.
    let insert_pos = result
        .find("precision highp float;")
        .map(|p| p + "precision highp float;".len())
        .or_else(|| result.find('\n').map(|p| p + 1))
        .unwrap_or(0);
    result.insert_str(insert_pos, "\nlayout(location = 0) out vec4 fragColor;\n");
    result
}

fn replace_builtin_names(source: &str) -> String {
    let mut result = source.to_string();

    // gl_FragDepthEXT -> gl_FragDepth (GLES 3.0 has gl_FragDepth built-in).
    static RE_FRAG_DEPTH_EXT: OnceLock<Regex> = OnceLock::new();
    let re_frag_depth_ext = RE_FRAG_DEPTH_EXT.get_or_init(|| word_regex("gl_FragDepthEXT"));
    result = replace_word(&result, re_frag_depth_ext, "gl_FragDepth");

    // ftransform() cannot be accurately translated without a full parser.
    if result.contains("ftransform()") {
        log::warn!(
            "[StringPass] ftransform() is not supported; replacing with identity placeholder"
        );
        result = result.replace(
            "ftransform()",
            "gl_Position = vec4(0.0, 0.0, 0.0, 1.0) /* ftransform not supported */",
        );
    }

    result
}

/// 构造匹配整个单词的正则 `\b{name}\b`（对 name 做 regex::escape）。
/// 供 replace_word 调用方预构造静态 Regex 时使用。
fn word_regex(name: &str) -> Regex {
    Regex::new(&format!(r"\b{}\b", regex::escape(name))).unwrap()
}

fn replace_word(source: &str, re: &Regex, to: &str) -> String {
    re.replace_all(source, to).into_owned()
}

/// 移除 `#line` 指令行。
///
/// string_pass 不经过 preprocess 阶段，GLSL ES 编译器对 `#line` 指令的支持有限，
/// 残留的 `#line` 指令可能导致编译警告或错误，需在此清除。
fn strip_line_directives(source: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // 匹配 `#line N` 或 `# line N`（允许 # 与 line 之间有空白）及其后的行内容
        Regex::new(r"(?m)^[ \t]*#[ \t]*line\b[^\n]*\n?").unwrap()
    });
    let result = re.replace_all(source, "").into_owned();
    if result.len() != source.len() {
        log::debug!("[StringPass] 已移除 #line 指令");
    }
    result
}

/// 移除 MC 的 `/*#version N*/` 注释行。
///
/// Minecraft 核心着色器中有时会包含 `/*#version 150*/` 形式的注释，
/// 该注释在 GLSL ES 上下文中无意义，移除以免干扰后续处理。
fn strip_mc_version_comment(source: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"/\*#version\s+\d+\s*\*/").unwrap());
    let result = re.replace_all(source, "").into_owned();
    if result.len() != source.len() {
        log::debug!("[StringPass] 已移除 /*#version N*/ 注释");
    }
    result
}

/// 定点修复：Minecraft `minecraft_sample_lightmap` 函数中的 ivec2/float 除法。
///
/// MC 核心光照贴图采样函数签名为 `vec4 minecraft_sample_lightmap(sampler2D, ivec2 uv)`，
/// 函数体内 `uv / 256.0` 是 ivec2/float 除法，在 OpenGL ES 中非法。
/// 此函数将 `uv / <float>` 替换为 `vec2(uv) / <float>`，仅作用于该函数内部。
///
/// 这是定点修复，不做通用类型推断，避免正则误替换风险。
fn fix_minecraft_lightmap(source: &str) -> String {
    // 快速检查：不包含目标函数时直接返回原文
    if !source.contains("minecraft_sample_lightmap") {
        return source.to_string();
    }

    // 匹配 minecraft_sample_lightmap 函数定义
    // 组1: 函数头（返回类型 + 函数名 + 参数列表 + 开括号）
    // 组2: 函数体（不含嵌套大括号，MC 光照函数体内无嵌套）
    // 组3: 闭括号
    static FUNC_RE: OnceLock<Regex> = OnceLock::new();
    let func_re = FUNC_RE.get_or_init(|| {
        Regex::new(r"(vec4\s+minecraft_sample_lightmap\s*\([^)]*\)\s*\{)([^}]*)(\})").unwrap()
    });

    // 从函数头提取 ivec2 参数名
    static IVEC2_RE: OnceLock<Regex> = OnceLock::new();
    let ivec2_re = IVEC2_RE.get_or_init(|| Regex::new(r"\bivec2\s+(\w+)").unwrap());

    func_re
        .replace_all(source, |caps: &regex::Captures| {
            let header = &caps[1];
            let body = &caps[2];

            // 从函数头中提取 ivec2 参数名
            let Some(param_caps) = ivec2_re.captures(header) else {
                log::warn!(
                    "[StringPass] minecraft_sample_lightmap 存在但未匹配到 ivec2 参数，跳过修复"
                );
                return caps[0].to_string();
            };

            let param = &param_caps[1];

            // 构造定点修复正则：<param> / <float_literal> → vec2(<param>) / <float_literal>
            // 仅匹配浮点字面量（必须含小数点或 f/F 后缀），避免误匹配整数除法
            let div_pattern = format!(r"\b{}\b\s*/\s*(\d+\.\d*[fF]?)", regex::escape(param));
            let div_re = match Regex::new(&div_pattern) {
                Ok(re) => re,
                Err(e) => {
                    log::warn!(
                        "[StringPass] 构造 ivec2/float 除法修复正则失败: {}，跳过修复",
                        e
                    );
                    return caps[0].to_string();
                }
            };

            let new_body = div_re.replace_all(body, |c: &regex::Captures| {
                format!("vec2({}) / {}", param, &c[1])
            });

            if new_body.as_ref() != body {
                log::debug!(
                    "[StringPass] 已修复 minecraft_sample_lightmap 中 ivec2/float 除法，参数: {}",
                    param
                );
            }

            format!("{}{}{}", header, new_body, &caps[3])
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── fix_minecraft_lightmap 测试 ──

    #[test]
    fn test_fix_minecraft_lightmap_basic() {
        // MC 1.21 光照贴图采样函数的典型崩溃模式
        let input = r#"vec4 minecraft_sample_lightmap(sampler2D lightMap, ivec2 uv) {
    return texture(lightMap, clamp(uv / 256.0, vec2(0.5 / 16.0), vec2(15.5 / 16.0)));
}"#;
        let result = fix_minecraft_lightmap(input);
        assert!(
            result.contains("vec2(uv) / 256.0"),
            "应将 uv / 256.0 转换为 vec2(uv) / 256.0，实际: {}",
            result
        );
        assert!(
            !result.contains("clamp(uv / 256.0"),
            "原始 ivec2/float 除法应已被替换，实际: {}",
            result
        );
        // clamp 的边界参数不应被误改
        assert!(
            result.contains("vec2(0.5 / 16.0)"),
            "clamp 的 vec2(0.5 / 16.0) 边界参数不应被修改"
        );
        assert!(
            result.contains("vec2(15.5 / 16.0)"),
            "clamp 的 vec2(15.5 / 16.0) 边界参数不应被修改"
        );
    }

    #[test]
    fn test_fix_minecraft_lightmap_no_function() {
        // 不包含 minecraft_sample_lightmap 时应原样返回
        let input = "void main() { vec2 v = vec2(1.0); }";
        let result = fix_minecraft_lightmap(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_fix_minecraft_lightmap_param_renamed() {
        // 参数名不是 uv 时也应正确提取并替换
        let input = r#"vec4 minecraft_sample_lightmap(sampler2D lightMap, ivec2 uvCoords) {
    return texture(lightMap, clamp(uvCoords / 256.0, vec2(0.0), vec2(1.0)));
}"#;
        let result = fix_minecraft_lightmap(input);
        assert!(
            result.contains("vec2(uvCoords) / 256.0"),
            "应使用实际参数名进行替换，实际: {}",
            result
        );
    }

    #[test]
    fn test_fix_minecraft_lightmap_no_ivec2_param() {
        // 函数存在但没有 ivec2 参数时不应崩溃，原样返回
        let input = r#"vec4 minecraft_sample_lightmap(sampler2D lightMap, vec2 uv) {
    return texture(lightMap, clamp(uv / 256.0, vec2(0.0), vec2(1.0)));
}"#;
        let result = fix_minecraft_lightmap(input);
        assert_eq!(result, input, "无 ivec2 参数时不应修改");
    }

    #[test]
    fn test_fix_minecraft_lightmap_multiple_divisions() {
        // 函数体内多处 ivec2/float 除法都应被修复
        let input = r#"vec4 minecraft_sample_lightmap(sampler2D lightMap, ivec2 uv) {
    vec2 a = uv / 256.0;
    vec2 b = uv / 16.0;
    return texture(lightMap, clamp(a, b, vec2(1.0)));
}"#;
        let result = fix_minecraft_lightmap(input);
        assert!(result.contains("vec2(uv) / 256.0"), "第一处除法应被修复");
        assert!(result.contains("vec2(uv) / 16.0"), "第二处除法应被修复");
    }

    #[test]
    fn test_fix_minecraft_lightmap_scoped_only() {
        // 修复应仅限于函数内部，不影响外部同名变量
        let input = r#"vec2 otherFunc(ivec2 uv) {
    return uv / 256.0;
}
vec4 minecraft_sample_lightmap(sampler2D lightMap, ivec2 uv) {
    return texture(lightMap, clamp(uv / 256.0, vec2(0.0), vec2(1.0)));
}"#;
        let result = fix_minecraft_lightmap(input);
        // otherFunc 中的 uv / 256.0 不应被修改（不在 minecraft_sample_lightmap 内）
        assert!(
            result.contains("return uv / 256.0;"),
            "外部函数中的 uv / 256.0 不应被修改，实际: {}",
            result
        );
        // minecraft_sample_lightmap 中的应被修复
        assert!(
            result.contains("vec2(uv) / 256.0"),
            "minecraft_sample_lightmap 内的除法应被修复"
        );
    }

    #[test]
    fn test_fix_minecraft_lightmap_integer_division_untouched() {
        // 整数除法（uv / 2）不应被修改，仅浮点除法才修复
        let input = r#"vec4 minecraft_sample_lightmap(sampler2D lightMap, ivec2 uv) {
    ivec2 half_uv = uv / 2;
    return texture(lightMap, clamp(uv / 256.0, vec2(0.0), vec2(1.0)));
}"#;
        let result = fix_minecraft_lightmap(input);
        assert!(result.contains("uv / 2"), "整数除法 uv / 2 不应被修改");
        assert!(result.contains("vec2(uv) / 256.0"), "浮点除法应被修复");
    }

    // ── strip_line_directives 测试 ──

    #[test]
    fn test_strip_line_directives_basic() {
        let input = "#version 330\n#line 0 2\nvoid main() {}\n";
        let result = strip_line_directives(input);
        assert!(!result.contains("#line"), "#line 指令应被移除");
        assert!(result.contains("#version 330"), "#version 应保留");
        assert!(result.contains("void main()"), "其他代码应保留");
    }

    #[test]
    fn test_strip_line_directives_multiple() {
        let input = "#line 1\nint a;\n#line 2\nint b;\n";
        let result = strip_line_directives(input);
        assert!(!result.contains("#line"), "所有 #line 指令应被移除");
        assert!(result.contains("int a;"), "代码应保留");
        assert!(result.contains("int b;"), "代码应保留");
    }

    #[test]
    fn test_strip_line_directives_with_space() {
        // # 与 line 之间有空白的情况
        let input = "# line 5\nvoid main() {}\n";
        let result = strip_line_directives(input);
        assert!(!result.contains("#line"), "# line 变体应被移除");
        assert!(!result.contains("# line"), "# line 变体应被移除");
    }

    #[test]
    fn test_strip_line_directives_none() {
        let input = "#version 300 es\nvoid main() {}\n";
        let result = strip_line_directives(input);
        assert_eq!(result, input, "无 #line 时应原样返回");
    }

    // ── strip_mc_version_comment 测试 ──

    #[test]
    fn test_strip_mc_version_comment_basic() {
        let input = "/*#version 150*/\nvoid main() {}\n";
        let result = strip_mc_version_comment(input);
        assert!(
            !result.contains("/*#version"),
            "/*#version N*/ 注释应被移除"
        );
        assert!(result.contains("void main()"), "其他代码应保留");
    }

    #[test]
    fn test_strip_mc_version_comment_with_space() {
        let input = "/*#version 150 */\nvoid main() {}\n";
        let result = strip_mc_version_comment(input);
        assert!(
            !result.contains("/*#version"),
            "带空格的 /*#version N */ 应被移除"
        );
    }

    #[test]
    fn test_strip_mc_version_comment_none() {
        let input = "// normal comment\nvoid main() {}\n";
        let result = strip_mc_version_comment(input);
        assert_eq!(result, input, "普通注释不应被移除");
    }

    // ── translate 集成测试（确保不回归） ──

    #[test]
    fn test_translate_mc_vertex_shader_lightmap() {
        // 模拟 MC 1.21 rendertype_solid vertex shader 的关键片段
        let input = r#"#version 150
/*#version 150*/
#line 0

uniform mat4 ModelViewMat;
uniform sampler2D Sampler2;
in ivec2 UV2;
in vec4 Color;
out vec4 vertexColor;

vec4 minecraft_sample_lightmap(sampler2D lightMap, ivec2 uv) {
    return texture(lightMap, clamp(uv / 256.0, vec2(0.5 / 16.0), vec2(15.5 / 16.0)));
}

void main() {
    vertexColor = Color * minecraft_sample_lightmap(Sampler2, UV2);
}
"#;
        let result = translate(input, GL_VERTEX_SHADER);
        // 应包含修复后的 vec2(uv) / 256.0
        assert!(
            result.contains("vec2(uv) / 256.0"),
            "translate 应修复 ivec2/float 除法，实际: {}",
            result
        );
        // #line 指令应被移除
        assert!(!result.contains("#line"), "#line 指令应被移除");
        // /*#version 150*/ 注释应被移除
        assert!(
            !result.contains("/*#version"),
            "/*#version N*/ 注释应被移除"
        );
        // 应有正确的 GLES 版本（string_pass 对桌面 GLSL 120..=460 输出 320 es，
        // 与 SPIR-V 翻译管线输出一致，避免 VS/FS 混合链接时 version mismatch）
        assert!(
            result.starts_with("#version 320 es"),
            "应以 #version 320 es 开头"
        );
        // precision 应被注入
        assert!(
            result.contains("precision highp float;"),
            "应注入 float precision"
        );
    }

    #[test]
    fn test_translate_already_gles_no_regression() {
        // 已是 GLSL ES 的源码应原样返回（早期退出路径）
        let input = "#version 300 es\nprecision highp float;\nvoid main() {}\n";
        let result = translate(input, GL_FRAGMENT_SHADER);
        assert_eq!(result, input, "已是 GLES 的源码不应被修改");
    }

    #[test]
    fn test_translate_simple_shader_no_lightmap() {
        // 不含 minecraft_sample_lightmap 的普通 shader 应正常翻译
        let input = "#version 330\nvoid main() { gl_Position = vec4(1.0); }\n";
        let result = translate(input, GL_VERTEX_SHADER);
        assert!(
            result.starts_with("#version 320 es"),
            "版本应被替换为 320 es"
        );
        // 不应有 vec2() 包装（没有 lightmap 函数）
        assert!(
            !result.contains("minecraft_sample_lightmap"),
            "不应出现 minecraft_sample_lightmap"
        );
    }
}
