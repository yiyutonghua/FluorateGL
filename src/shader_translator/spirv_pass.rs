use naga::back::glsl::{Error as GlslError, Options as GlslOptions, PipelineOptions, Version, WriterFlags};
use naga::front::spv::Options as SpvOptions;
use naga::valid::{Capabilities, ValidationFlags};

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;
const GL_COMPUTE_SHADER: u32 = 0x91B9;

/// Translate desktop GLSL to GLSL ES via shaderc (GLSL -> SPIR-V) and
/// naga (SPIR-V -> GLSL ES).
///
/// Returns `None` when any pipeline step fails. Geometry and tessellation
/// stages are rejected immediately because naga 30 does not model them.
pub fn translate(source: &str, stage: u32) -> Option<String> {
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
            None
        }
    }
}

fn translate_internal(source: &str, stage: u32) -> Option<String> {
    let stage_name = stage_name(stage);
    let version_line = extract_version(source).unwrap_or("unknown");
    log::info!(
        "[ShaderTranslator] SPIR-V translate start: stage={} (0x{:04X}), version={}",
        stage_name, stage, version_line
    );

    // naga 30 only supports vertex/fragment/compute stages, so avoid wasting
    // time invoking shaderc for stages that can never succeed.
    if is_unsupported_stage(stage) {
        log::warn!(
            "[ShaderTranslator] stage 0x{:04X} is not supported by naga 30, skipping SPIR-V path",
            stage
        );
        return None;
    }

    let spv = compile_to_spirv(source, stage)?;
    let module = parse_spirv(&spv)?;
    let info = validate_module(&module)?;
    let glsl_stage = naga_stage(stage)?;

    // Try GLES versions from most compatible to least compatible.
    for gles_version in gles_version_candidates(source) {
        match write_gles(&module, &info, glsl_stage, gles_version) {
            Ok(src) => {
                log::info!(
                    "[ShaderTranslator] SPIR-V translate success: stage={}, version={}",
                    stage_name, gles_version
                );
                return Some(src);
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
    None
}

fn compile_to_spirv(source: &str, stage: u32) -> Option<Vec<u32>> {
    let compiler = shaderc::Compiler::new().ok()?;
    let mut options = shaderc::CompileOptions::new().ok()?;

    // Always use Vulkan semantics: naga's spirv-in is designed and tested for
    // SPIR-V produced under Vulkan rules.
    options.set_target_env(shaderc::TargetEnv::Vulkan, shaderc::EnvVersion::Vulkan1_0 as u32);
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);

    // Desktop GLSL (e.g. #version 150 core) rarely declares explicit locations
    // on stage inputs/outputs or bindings on uniforms. Vulkan SPIR-V requires
    // them, so let shaderc auto-generate them.
    options.set_auto_map_locations(true);
    options.set_auto_bind_uniforms(true);
    options.set_suppress_warnings();
    // Do not generate debug info: it bloats the SPIR-V and can trip up naga.

    let kind = shader_kind(stage);
    let file_name = format!("shader_{:04X}.glsl", kind as u32);

    compiler
        .compile_into_spirv(source, kind, &file_name, "main", Some(&options))
        .map_err(|e| {
            log::warn!("[ShaderTranslator] shaderc compile failed: {}", e);
        })
        .ok()
        .map(|a| a.as_binary().to_vec())
}

fn shader_kind(stage: u32) -> shaderc::ShaderKind {
    match stage {
        GL_VERTEX_SHADER => shaderc::ShaderKind::Vertex,
        GL_FRAGMENT_SHADER => shaderc::ShaderKind::Fragment,
        GL_COMPUTE_SHADER => shaderc::ShaderKind::Compute,
        _ => shaderc::ShaderKind::InferFromSource,
    }
}

fn is_unsupported_stage(stage: u32) -> bool {
    matches!(
        stage,
        GL_GEOMETRY_SHADER | GL_TESS_CONTROL_SHADER | GL_TESS_EVALUATION_SHADER
    )
}

fn parse_spirv(spv: &[u32]) -> Option<naga::Module> {
    // Use default spirv-in options so that adjust_coordinate_space is enabled.
    // This converts Vulkan SPIR-V coordinates into naga IR / OpenGL coordinates.
    naga::front::spv::parse_u8_slice(bytemuck::cast_slice(spv), &SpvOptions::default())
        .map_err(|e| {
            log::warn!("[ShaderTranslator] naga spirv-in failed: {:?}", e);
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
