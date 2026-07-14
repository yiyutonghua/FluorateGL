const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
#[allow(dead_code)]
const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
#[allow(dead_code)]
const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
#[allow(dead_code)]
const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;

/// A lightweight string-based GLSL -> GLSL ES translator.
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

    // Legacy GLSL (1.20/1.30) uses attribute/varying/texture2D.
    output = replace_legacy_syntax(&output, stage);

    // GLSL ES requires explicit precision for float.
    output = inject_precision(&output, stage);

    // gl_FragColor is not built-in in GLSL ES 300.
    output = replace_frag_color(&output);

    // Some built-in names differ or are unavailable.
    output = replace_builtin_names(&output);

    output
}

fn replace_version(source: &str) -> String {
    let mut result = source.to_string();

    // Common desktop profiles -> GLES 300.
    let replacements: &[(&str, &str)] = &[
        ("#version 150 core", "#version 300 es"),
        ("#version 150", "#version 300 es"),
        ("#version 130 core", "#version 300 es"),
        ("#version 130", "#version 300 es"),
        ("#version 120", "#version 300 es"),
        ("#version 330 core", "#version 300 es"),
        ("#version 330", "#version 300 es"),
        ("#version 400 core", "#version 300 es"),
        ("#version 400", "#version 300 es"),
        ("#version 410 core", "#version 300 es"),
        ("#version 410", "#version 300 es"),
        ("#version 420 core", "#version 300 es"),
        ("#version 420", "#version 300 es"),
        ("#version 430 core", "#version 300 es"),
        ("#version 430", "#version 300 es"),
        ("#version 440 core", "#version 300 es"),
        ("#version 440", "#version 300 es"),
        ("#version 450 core", "#version 320 es"),
        ("#version 450", "#version 320 es"),
        ("#version 460 core", "#version 320 es"),
        ("#version 460", "#version 320 es"),
    ];

    for (from, to) in replacements {
        result = result.replace(from, to);
    }

    result
}

fn replace_legacy_syntax(source: &str, stage: u32) -> String {
    let mut result = source.to_string();

    // Only legacy GLSL (<=1.30) uses attribute/varying.
    if stage == GL_VERTEX_SHADER {
        result = result.replace("attribute ", "in ");
        result = result.replace("varying ", "out ");
    } else if stage == GL_FRAGMENT_SHADER {
        result = result.replace("varying ", "in ");
    }

    // Legacy texture lookup functions.
    result = result.replace("texture1D", "texture");
    result = result.replace("texture2D", "texture");
    result = result.replace("texture3D", "texture");
    result = result.replace("textureCube", "texture");
    result = result.replace("texture1DLod", "textureLod");
    result = result.replace("texture2DLod", "textureLod");
    result = result.replace("texture3DLod", "textureLod");
    result = result.replace("textureCubeLod", "textureLod");

    result
}

fn inject_precision(source: &str, _stage: u32) -> String {
    // Already has precision declaration?
    if source.contains("precision ") {
        return source.to_string();
    }

    // Insert after the #version line.
    if let Some(pos) = source.find('\n') {
        let mut result = source[..pos + 1].to_string();
        result.push_str("precision highp float;\n");
        result.push_str(&source[pos + 1..]);
        result
    } else {
        format!("precision highp float;\n{}", source)
    }
}

fn replace_frag_color(source: &str) -> String {
    if !source.contains("gl_FragColor") {
        return source.to_string();
    }

    let mut result = source.replace("gl_FragColor", "fragColor");

    // Add the output declaration if the shader does not already define one.
    if !result.contains("out vec4 fragColor")
        && !result.contains("layout(")
        && !result.contains("out ")
    {
        // Insert after the precision line or #version line.
        let insert_pos = result
            .find("precision highp float;")
            .map(|p| p + "precision highp float;".len())
            .or_else(|| result.find('\n').map(|p| p + 1))
            .unwrap_or(0);
        result.insert_str(insert_pos, "\nout vec4 fragColor;\n");
    } else if !result.contains("out vec4 fragColor") && result.contains("out ") {
        // Some other out exists; still need fragColor output if gl_FragColor was used.
        let insert_pos = result
            .find("precision highp float;")
            .map(|p| p + "precision highp float;".len())
            .or_else(|| result.find('\n').map(|p| p + 1))
            .unwrap_or(0);
        result.insert_str(insert_pos, "\nout vec4 fragColor;\n");
    }

    result
}

fn replace_builtin_names(source: &str) -> String {
    let mut result = source.to_string();

    // gl_FragDepthEXT -> gl_FragDepth (GLES 3.0 has gl_FragDepth built-in).
    result = result.replace("gl_FragDepthEXT", "gl_FragDepth");

    // Some shaders use ftransform() which does not exist in GLSL ES.
    result = result.replace("ftransform()", "gl_Position = gl_ModelViewProjectionMatrix * gl_Vertex");

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_replace() {
        let src = "#version 150 core\nvoid main() {}\n";
        let out = translate(src, GL_VERTEX_SHADER);
        assert!(out.contains("#version 300 es"));
    }

    #[test]
    fn test_precision_injected() {
        let src = "#version 150 core\nvoid main() {}\n";
        let out = translate(src, GL_FRAGMENT_SHADER);
        assert!(out.contains("precision highp float;"));
    }

    #[test]
    fn test_legacy_varying() {
        let src = "#version 120\nvarying vec2 uv;\nvoid main() {}\n";
        let out = translate(src, GL_FRAGMENT_SHADER);
        assert!(out.contains("in vec2 uv"));
    }

    #[test]
    fn test_texture2d() {
        let src = "#version 120\nvoid main() { vec4 c = texture2D(tex, uv); }\n";
        let out = translate(src, GL_FRAGMENT_SHADER);
        assert!(out.contains("texture(tex, uv)"));
        assert!(!out.contains("texture2D"));
    }
}
