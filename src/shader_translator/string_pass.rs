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
            450 | 460 => "#version 320 es",
            120..=440 => "#version 300 es",
            _ => "#version 300 es",
        };
        target.to_string()
    })
    .into_owned()
}

fn replace_legacy_syntax(source: &str, stage: u32) -> String {
    let mut result = source.to_string();

    // Only legacy GLSL (<=1.30) uses attribute/varying.
    if stage == GL_VERTEX_SHADER {
        result = replace_word(&result, "attribute", "in");
        result = replace_word(&result, "varying", "out");
    } else if stage == GL_FRAGMENT_SHADER {
        result = replace_word(&result, "varying", "in");
    }

    // Legacy texture lookup functions. Order matters: longer names first.
    let replacements: &[(&str, &str)] = &[
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

    for (from, to) in replacements {
        result = replace_word(&result, from, to);
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
    result = replace_word(&result, "gl_FragDepthEXT", "gl_FragDepth");

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

fn replace_word(source: &str, from: &str, to: &str) -> String {
    let re = Regex::new(&format!(r"\b{}\b", regex::escape(from))).unwrap();
    re.replace_all(source, to).into_owned()
}
