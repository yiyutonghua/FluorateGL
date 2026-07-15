use naga::back::glsl::{Error as GlslError, Options as GlslOptions, PipelineOptions, Version, WriterFlags};
use naga::front::spv::Options as SpvOptions;
use naga::valid::{Capabilities, ValidationFlags};

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;
const GL_COMPUTE_SHADER: u32 = 0x91B9;

/// Result of attempting to translate a desktop GLSL shader for GLES.
#[derive(Debug, Clone)]
pub enum TranslationResult {
    /// A translated GLSL ES source string ready to upload.
    Translated(String),
    /// The original source should be passed through unchanged.
    /// Used for geometry/tessellation when the GLES driver supports the
    /// corresponding extension.
    PassThrough,
    /// Translation failed and there is no usable output.
    Failed,
}

/// Translate desktop GLSL to GLSL ES via shaderc (GLSL -> SPIR-V) and
/// naga (SPIR-V -> GLSL ES).
///
/// Geometry and tessellation shaders cannot be represented in naga 30's IR.
/// When the GLES driver advertises the matching extension, the original source
/// is returned as [`TranslationResult::PassThrough`] so the driver can compile
/// it directly. Otherwise the shader is reported as unsupported.
pub fn translate(source: &str, stage: u32) -> TranslationResult {
    // naga 30 has known internal panics on some SPIR-V inputs (e.g. typifier
    // index out of bounds). Wrap the whole pipeline in catch_unwind so that a
    // translator bug does not abort the host process.
    match std::panic::catch_unwind(|| translate_internal(source, stage)) {
        Ok(result) => result,
        Err(_) => {
            log::error!(
                "[ShaderTranslator] SPIR-V pipeline panicked for stage 0x{:04X}; skipping",
                stage
            );
            TranslationResult::Failed
        }
    }
}

fn translate_internal(source: &str, stage: u32) -> TranslationResult {
    let stage_name = stage_name(stage);
    let version_line = extract_version(source).unwrap_or("unknown");
    log::info!(
        "[ShaderTranslator] SPIR-V translate start: stage={} (0x{:04X}), version={}",
        stage_name, stage, version_line
    );

    // naga 30 only supports vertex/fragment/compute stages. Geometry and
    // tessellation can only work if the GLES driver supports the matching
    // desktop extension and we pass the original GLSL through.
    if is_unsupported_stage(stage) {
        return if should_pass_through_stage(stage) {
            log::info!(
                "[ShaderTranslator] stage 0x{:04X} supported by driver extension; passing original source through",
                stage
            );
            TranslationResult::PassThrough
        } else {
            log::warn!(
                "[ShaderTranslator] stage 0x{:04X} not supported by driver and cannot be translated; failing",
                stage
            );
            TranslationResult::Failed
        };
    }

    match compile_to_spirv(source, stage) {
        Some(spv) => match parse_spirv(&spv) {
            Some(module) => match validate_module(&module) {
                Some(info) => {
                    let glsl_stage = match naga_stage(stage) {
                        Some(s) => s,
                        None => return TranslationResult::Failed,
                    };

                    // Try GLES versions from most compatible to least compatible.
                    for gles_version in gles_version_candidates(source) {
                        match write_gles(&module, &info, glsl_stage, gles_version) {
                            Ok(src) => {
                                log::info!(
                                    "[ShaderTranslator] SPIR-V translate success: stage={}, version={}",
                                    stage_name, gles_version
                                );
                                log::debug!("[ShaderTranslator] translated GLSL ES:\n{}", src);
                                return TranslationResult::Translated(src);
                            }
                            Err(e) => {
                                log::warn!(
                                    "[ShaderTranslator] GLES {} write failed for stage {}: {:?}",
                                    gles_version, stage_name, e
                                );
                            }
                        }
                    }

                    log::warn!(
                        "[ShaderTranslator] all GLES versions failed for shader stage {}",
                        stage_name
                    );
                    TranslationResult::Failed
                }
                None => TranslationResult::Failed,
            },
            None => TranslationResult::Failed,
        },
        None => TranslationResult::Failed,
    }
}

fn compile_to_spirv(source: &str, stage: u32) -> Option<Vec<u32>> {
    let compiler = shaderc::Compiler::new().ok()?;
    let mut options = shaderc::CompileOptions::new().ok()?;

    // Always use Vulkan semantics: naga's spirv-in is designed and tested for
    // SPIR-V produced under Vulkan rules.
    options.set_target_env(shaderc::TargetEnv::Vulkan, shaderc::EnvVersion::Vulkan1_0 as u32);
    // shaderc's Performance optimization runs spirv-opt which can introduce
    // patterns that naga 30's spirv-in does not handle (e.g. folded
    // OpSampledImage). Keep optimizations off for now.
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);

    // Desktop GLSL (e.g. #version 150 core) rarely declares explicit locations
    // on stage inputs/outputs or bindings on uniforms. Vulkan SPIR-V requires
    // them, so let shaderc auto-generate them.
    options.set_auto_map_locations(true);
    options.set_auto_bind_uniforms(true);
    options.set_suppress_warnings();
    // shaderc generates line/debug instructions by default when targetting
    // Vulkan; naga 30's spirv-in does not support OpLine, so leave debug info
    // disabled (this is the default).

    let kind = shader_kind(stage);
    let file_name = format!("shader_{:04X}.glsl", kind as u32);

    // naga 30's spirv-in cannot parse SPIR-V for combined GLSL samplers
    // (sampler2D etc.). Split them into Vulkan-style separate image/sampler
    // pairs first.
    let split = split_combined_samplers(source);

    // Vulkan GLSL requires non-opaque uniforms to live inside a uniform block.
    // Wrap standalone uniforms (e.g. `uniform mat4 MVP;`) in an anonymous block
    // so the API name (`glGetUniformLocation("MVP")`) remains valid.
    let prepared = prepare_vulkan_glsl(&split);

    log::debug!(
        "[ShaderTranslator] prepared Vulkan GLSL for shaderc:\n{}",
        prepared
    );

    compiler
        .compile_into_spirv(&prepared, kind, &file_name, "main", Some(&options))
        .map_err(|e| {
            log::warn!("[ShaderTranslator] shaderc compile failed: {}", e);
        })
        .ok()
        .map(|a| a.as_binary().to_vec())
}

/// Mapping from combined GLSL sampler types to the corresponding Vulkan
/// separate image type. The sampler helper always uses the `sampler` type.
const COMBINED_SAMPLER_TYPES: &[(&str, &str)] = &[
    ("sampler1D", "texture1D"),
    ("sampler2D", "texture2D"),
    ("sampler3D", "texture3D"),
    ("samplerCube", "textureCube"),
    ("sampler1DArray", "texture1DArray"),
    ("sampler2DArray", "texture2DArray"),
    ("samplerCubeArray", "textureCubeArray"),
    ("sampler2DRect", "texture2DRect"),
    ("samplerBuffer", "textureBuffer"),
    ("sampler2DMS", "texture2DMS"),
    ("sampler2DMSArray", "texture2DMSArray"),
    ("isampler1D", "itexture1D"),
    ("isampler2D", "itexture2D"),
    ("isampler3D", "itexture3D"),
    ("isamplerCube", "itextureCube"),
    ("isampler1DArray", "itexture1DArray"),
    ("isampler2DArray", "itexture2DArray"),
    ("isamplerCubeArray", "itextureCubeArray"),
    ("usampler1D", "utexture1D"),
    ("usampler2D", "utexture2D"),
    ("usampler3D", "utexture3D"),
    ("usamplerCube", "utextureCube"),
    ("usampler1DArray", "utexture1DArray"),
    ("usampler2DArray", "utexture2DArray"),
    ("usamplerCubeArray", "utextureCubeArray"),
    ("sampler2DShadow", "texture2D"),
    ("samplerCubeShadow", "textureCube"),
    ("sampler2DArrayShadow", "texture2DArray"),
];

/// Returns the SPIR-V constructor name for a combined sampler type (e.g.
/// `sampler2D` for a 2D colour sampler, `sampler2DShadow` for a shadow
/// sampler).
fn sampler_constructor(combined_type: &str) -> &str {
    combined_type
}

/// naga 30's spirv-in cannot parse SPIR-V produced from GLSL `sampler2D`
/// combined samplers (it fails with InvalidId on the OpLoad of the sampled
/// image). Convert combined samplers into Vulkan-style separate image/sampler
/// pairs. The original sampler name is kept for the image; a helper sampler is
/// introduced. Texture calls are rewritten to use the helper. naga's GLSL
/// backend will recombine them into a single `sampler2D` for GLES output.
fn split_combined_samplers(source: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static UNIFORM_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?m)^\s*(?P<layout>layout\s*\([^)]*\)\s+)?uniform\s+(?P<type>[A-Za-z_][A-Za-z0-9_]*)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*(?P<array>\[[^\]]*\])?\s*;\s*$"
        ).unwrap()
    });

    let mut result = source.to_string();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    // (name, constructor_name, helper_sampler_name)
    let mut samplers: Vec<(String, String, String)> = Vec::new();

    for cap in UNIFORM_RE.captures_iter(source) {
        let type_name = &cap["type"];
        log::debug!(
            "[ShaderTranslator] split_combined_samplers saw uniform: type={} name={}",
            type_name, &cap["name"]
        );
        let Some(&(_, image_type)) = COMBINED_SAMPLER_TYPES.iter().find(|&&(t, _)| t == type_name) else {
            continue;
        };

        let name = cap["name"].to_string();
        let ctor = sampler_constructor(type_name).to_string();
        let sampler_helper = format!("_fluorategl_{}_smp", name);
        let layout = cap.name("layout").map(|m| m.as_str()).unwrap_or("");
        let array = cap.name("array").map(|m| m.as_str()).unwrap_or("");

        let new_decl = format!(
            "{layout}uniform {image_type} {name}{array};\nuniform sampler {sampler_helper};\n",
            layout = layout,
            image_type = image_type,
            name = name,
            array = array,
            sampler_helper = sampler_helper
        );

        let range = cap.get(0).unwrap().range();
        log::debug!(
            "[ShaderTranslator] split_combined_samplers replacing range {:?} with:\n{}",
            range, new_decl
        );
        replacements.push((range.start, range.end, new_decl));
        samplers.push((name, ctor, sampler_helper));
    }

    // Apply declaration replacements from the end so offsets remain valid.
    for (start, end, text) in replacements.into_iter().rev() {
        result.replace_range(start..end, &text);
    }

    // Rewrite texture function calls for each split sampler.
    for (name, ctor, helper) in samplers {
        result = rewrite_sampler_uses(&result, &name, &ctor, &helper);
    }

    result
}

/// Texture-like functions whose first argument is a combined sampler. We keep
/// this list conservative; additional functions can be added as needed.
const SAMPLER_FUNCTIONS: &[&str] = &[
    "texture",
    "textureLod",
    "textureProj",
    "textureProjLod",
    "textureOffset",
    "textureLodOffset",
    "textureProjOffset",
    "textureProjLodOffset",
    "textureSize",
    "texelFetch",
    "texelFetchOffset",
];

/// Rewrite calls like `texture(Tex, uv)` into
/// `texture(sampler2D(Tex, _fluorategl_Tex_smp), uv)`.
fn rewrite_sampler_uses(source: &str, name: &str, ctor: &str, helper: &str) -> String {
    use regex::Regex;

    // Build a regex that matches any of the sampler functions, followed by a
    // parenthesised argument list whose first argument is exactly `name`.
    // The argument list is matched conservatively to handle nested parens up
    // to one level deep (e.g. vec2 constructors).
    let funcs = SAMPLER_FUNCTIONS.join("|");
    let pattern = format!(
        r"(?P<func>\b(?:{})\s*\()\s*(?P<name>\b{}\b)\s*(?P<rest>(?:[^()]|\([^)]*\))*)\)",
        funcs,
        regex::escape(name)
    );
    let re = Regex::new(&pattern).unwrap();

    re.replace_all(source, |caps: &regex::Captures| {
        let func = &caps["func"];
        let rest = &caps["rest"];
        format!("{}{}({}, {}){})", func, ctor, name, helper, rest)
    })
    .into_owned()
}

/// Wrap non-opaque standalone uniform declarations in an anonymous std140 block.
/// Opaque uniforms (samplers, images, atomic counters) and existing uniform
/// blocks are left untouched.
fn prepare_vulkan_glsl(source: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static UNIFORM_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?m)^\s*uniform\s+(?P<type>[A-Za-z_][A-Za-z0-9_]*)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*(?P<array>\[[^\]]*\])?\s*;\s*$"
        ).unwrap()
    });

    // Types that are opaque in GLSL and cannot be placed inside a uniform block.
    // This now includes the separate Vulkan image/sampler types introduced by
    // split_combined_samplers.
    const OPAQUE_TYPES: &[&str] = &[
        "sampler1D", "sampler2D", "sampler3D", "samplerCube",
        "sampler1DArray", "sampler2DArray", "samplerCubeArray",
        "sampler2DRect", "samplerBuffer", "sampler2DMS", "sampler2DMSArray",
        "isampler1D", "isampler2D", "isampler3D", "isamplerCube",
        "usampler1D", "usampler2D", "usampler3D", "usamplerCube",
        "image1D", "image2D", "image3D", "imageCube", "imageBuffer",
        "image1DArray", "image2DArray", "imageCubeArray", "image2DMS", "image2DMSArray",
        "iimage1D", "iimage2D", "iimage3D", "uimage1D", "uimage2D", "uimage3D",
        "atomic_uint", "samplerExternalOES",
        // Vulkan separate image/sampler types.
        "texture1D", "texture2D", "texture3D", "textureCube",
        "texture1DArray", "texture2DArray", "textureCubeArray",
        "texture2DRect", "textureBuffer", "texture2DMS", "texture2DMSArray",
        "itexture1D", "itexture2D", "itexture3D", "itextureCube",
        "itexture1DArray", "itexture2DArray", "itextureCubeArray",
        "utexture1D", "utexture2D", "utexture3D", "utextureCube",
        "utexture1DArray", "utexture2DArray", "utextureCubeArray",
        "sampler",
    ];

    let mut standalone = Vec::new();
    for cap in UNIFORM_RE.captures_iter(source) {
        let type_name = &cap["type"];
        if OPAQUE_TYPES.contains(&type_name) {
            continue;
        }
        // Skip declarations that already live inside a uniform block. A uniform
        // block starts with a line ending in '{'.
        let start = cap.get(0).unwrap().start();
        let prefix = &source[..start];
        if let Some(prev_line) = prefix.lines().filter(|l| !l.trim().is_empty()).last() {
            if prev_line.trim().ends_with('{') {
                continue;
            }
        }

        let decl = format!(
            "    {} {}{};",
            type_name,
            &cap["name"],
            cap.name("array").map(|m| m.as_str()).unwrap_or("")
        );
        standalone.push((cap.get(0).unwrap().range(), decl));
    }

    if standalone.is_empty() {
        return source.to_string();
    }

    // Replace each standalone uniform with a placeholder comment, and collect
    // the field declarations into a single anonymous uniform block inserted
    // immediately after the #version directive.
    let mut result = source.to_string();
    for (range, _) in standalone.iter().rev() {
        result.replace_range(range.clone(), "// (moved to _fluorategl_uniforms block)\n");
    }

    let block_body: String = standalone.into_iter().map(|(_, decl)| decl + "\n").collect();
    let block = format!(
        "layout(std140) uniform _fluorategl_uniforms {{\n{}}};\n",
        block_body
    );

    if let Some(pos) = result.find('\n') {
        let insert_at = result[pos..]
            .find('\n')
            .map(|p| pos + p + 1)
            .unwrap_or(pos + 1);
        result.insert_str(insert_at, &block);
    } else {
        result.insert_str(0, &block);
    }

    result
}

fn shader_kind(stage: u32) -> shaderc::ShaderKind {
    match stage {
        GL_VERTEX_SHADER => shaderc::ShaderKind::Vertex,
        GL_FRAGMENT_SHADER => shaderc::ShaderKind::Fragment,
        GL_COMPUTE_SHADER => shaderc::ShaderKind::Compute,
        GL_GEOMETRY_SHADER => shaderc::ShaderKind::Geometry,
        GL_TESS_CONTROL_SHADER => shaderc::ShaderKind::TessControl,
        GL_TESS_EVALUATION_SHADER => shaderc::ShaderKind::TessEvaluation,
        _ => shaderc::ShaderKind::InferFromSource,
    }
}

/// Stages that naga 30 cannot model at all.
fn is_unsupported_stage(stage: u32) -> bool {
    matches!(
        stage,
        GL_GEOMETRY_SHADER | GL_TESS_CONTROL_SHADER | GL_TESS_EVALUATION_SHADER
    )
}

/// Whether to pass the original source through for a stage that naga cannot
/// model. This depends on the GLES driver exposing the matching extension.
fn should_pass_through_stage(stage: u32) -> bool {
    let Some(caps) = crate::backend::gles_caps::get() else {
        return false;
    };

    match stage {
        GL_GEOMETRY_SHADER => {
            caps.has_extension("GL_EXT_geometry_shader")
                || caps.has_extension("GL_OES_geometry_shader")
        }
        GL_TESS_CONTROL_SHADER | GL_TESS_EVALUATION_SHADER => {
            caps.has_extension("GL_EXT_tessellation_shader")
                || caps.has_extension("GL_OES_tessellation_shader")
        }
        _ => false,
    }
}

fn parse_spirv(spv: &[u32]) -> Option<naga::Module> {
    // Use default spirv-in options so that adjust_coordinate_space is enabled.
    // This converts Vulkan SPIR-V coordinates into naga IR / OpenGL coordinates.
    naga::front::spv::parse_u8_slice(bytemuck::cast_slice(spv), &SpvOptions::default())
        .map_err(|e| {
            log::warn!("[ShaderTranslator] naga spirv-in failed: {:?}", e);
            if log::log_enabled!(log::Level::Debug) {
                let words: Vec<String> = spv.iter().map(|w| format!("{:08X}", w)).collect();
                log::debug!("[ShaderTranslator] failing SPIR-V ({} words): {}", spv.len(), words.join(" "));
            }
        })
        .ok()
}

fn validate_module(module: &naga::Module) -> Option<naga::valid::ModuleInfo> {
    let capabilities = build_capabilities();
    naga::valid::Validator::new(ValidationFlags::all(), capabilities)
        .validate(module)
        .map_err(|e| {
            log::warn!(
                "[ShaderTranslator] naga validation failed (capabilities={:?}): {:?}",
                capabilities, e
            );
        })
        .ok()
}

fn write_gles(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    version: u16,
) -> Result<String, GlslError> {
    let mut output = String::new();
    let options = GlslOptions {
        version: Version::new_gles(version),
        // WriterFlags::empty() keeps the output in OpenGL/GLES coordinate space.
        // Do not combine with spirv-in's default adjust_coordinate_space, as that
        // would cause a double flip.
        writer_flags: WriterFlags::empty(),
        binding_map: Default::default(),
        zero_initialize_workgroup_memory: true,
    };
    let pipeline_options = PipelineOptions {
        shader_stage: stage,
        entry_point: "main".to_string(),
        multiview: None,
    };

    let mut writer = naga::back::glsl::Writer::new(
        &mut output,
        module,
        info,
        &options,
        &pipeline_options,
        naga::proc::BoundsCheckPolicies::default(),
    )?;
    writer.write()?;
    Ok(output)
}

/// Build a capability set matching the actual GLES implementation.
/// Capabilities must be runtime-derived from the GLES version and extensions.
fn build_capabilities() -> Capabilities {
    let mut caps = Capabilities::empty();

    // Base capabilities supported by all GLES 3.x implementations.
    caps |= Capabilities::IMMEDIATES;
    caps |= Capabilities::TEXTURE_AND_SAMPLER_BINDING_ARRAY;
    caps |= Capabilities::BUFFER_BINDING_ARRAY;
    caps |= Capabilities::EARLY_DEPTH_TEST;

    let Some(gles) = crate::backend::gles_caps::get() else {
        // No caps available yet: be conservative and return the base set.
        log::warn!("[ShaderTranslator] GLES caps not available, using conservative capability set");
        return caps;
    };

    // Storage buffers / images require ES 3.1+.
    if gles.is_es31_plus() {
        caps |= Capabilities::STORAGE_BUFFER_BINDING_ARRAY;
        caps |= Capabilities::STORAGE_TEXTURE_BINDING_ARRAY;
    }

    // Cube array textures are core in ES 3.2, or available via extension.
    if gles.is_es32() || gles.has_extension("GL_EXT_texture_cube_map_array") {
        caps |= Capabilities::CUBE_ARRAY_TEXTURES;
    }

    // Clip/cull distances require the EXT extension on GLES.
    if gles.has_extension("GL_EXT_clip_cull_distance") {
        caps |= Capabilities::CLIP_DISTANCES;
        caps |= Capabilities::CULL_DISTANCE;
    }

    // gpu_shader5 enables non-uniform indexing of binding arrays.
    if gles.has_extension("GL_EXT_gpu_shader5") || gles.has_extension("GL_OES_gpu_shader5") {
        caps |= Capabilities::TEXTURE_AND_SAMPLER_BINDING_ARRAY_NON_UNIFORM_INDEXING;
        caps |= Capabilities::BUFFER_BINDING_ARRAY_NON_UNIFORM_INDEXING;
        caps |= Capabilities::STORAGE_TEXTURE_BINDING_ARRAY_NON_UNIFORM_INDEXING;
        caps |= Capabilities::STORAGE_BUFFER_BINDING_ARRAY_NON_UNIFORM_INDEXING;
    }

    caps
}

fn gles_version_candidates(source: &str) -> Vec<u16> {
    let desktop_version = extract_version(source)
        .and_then(|line| {
            line.split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<u32>().ok())
        })
        .unwrap_or(150);

    match desktop_version {
        460 | 450 | 440 => vec![300, 310, 320],
        430 | 420 | 410 | 400 | 330 => vec![300, 310],
        _ => vec![300],
    }
}

fn naga_stage(stage: u32) -> Option<naga::ShaderStage> {
    match stage {
        GL_VERTEX_SHADER => Some(naga::ShaderStage::Vertex),
        GL_FRAGMENT_SHADER => Some(naga::ShaderStage::Fragment),
        GL_COMPUTE_SHADER => Some(naga::ShaderStage::Compute),
        _ => {
            log::warn!("[ShaderTranslator] unknown shader stage 0x{:04X}", stage);
            None
        }
    }
}

fn stage_name(stage: u32) -> &'static str {
    match stage {
        GL_VERTEX_SHADER => "vertex",
        GL_FRAGMENT_SHADER => "fragment",
        GL_GEOMETRY_SHADER => "geometry",
        GL_TESS_CONTROL_SHADER => "tess_control",
        GL_TESS_EVALUATION_SHADER => "tess_eval",
        GL_COMPUTE_SHADER => "compute",
        _ => "unknown",
    }
}

fn extract_version(source: &str) -> Option<&str> {
    source
        .lines()
        .find(|l| l.trim_start().starts_with("#version"))
}
