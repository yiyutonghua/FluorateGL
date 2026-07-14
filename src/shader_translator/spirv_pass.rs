use naga::back::glsl::{Options as GlslOptions, PipelineOptions, WriterFlags};
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
/// Returns `None` when any pipeline step fails. Callers should fall back to a
/// simpler string-based translator.
pub fn translate(source: &str, stage: u32) -> Option<String> {
    let stage_name = stage_name(stage);
    let version_line = extract_version(source).unwrap_or("unknown");
    log::debug!(
        "[ShaderTranslator] SPIR-V translate start: stage={} (0x{:04X}), version={}",
        stage_name, stage, version_line
    );

    let spv = compile_to_spirv(source, stage)?;
    let gles = spirv_to_gles(&spv, stage, source)?;

    log::debug!(
        "[ShaderTranslator] SPIR-V translate success: stage={}, output_len={}",
        stage_name, gles.len()
    );
    Some(gles)
}

fn compile_to_spirv(source: &str, stage: u32) -> Option<Vec<u32>> {
    let kind = shader_kind(stage);

    // First try OpenGL semantics (closest to the original desktop GLSL).
    if let Some(spv) = try_compile(source, kind, shaderc::TargetEnv::OpenGL, shaderc::EnvVersion::OpenGL4_5 as u32) {
        return Some(spv);
    }

    // Fallback to Vulkan semantics, which naga's spirv-in understands better.
    if let Some(spv) = try_compile(
        source,
        kind,
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_0 as u32,
    ) {
        log::info!("[ShaderTranslator] shaderc compiled with Vulkan target fallback");
        return Some(spv);
    }

    None
}

fn try_compile(
    source: &str,
    kind: shaderc::ShaderKind,
    env: shaderc::TargetEnv,
    version: u32,
) -> Option<Vec<u32>> {
    let compiler = shaderc::Compiler::new().ok()?;
    let mut options = shaderc::CompileOptions::new().ok()?;

    options.set_target_env(env, version);
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);
    options.set_generate_debug_info();

    let file_name = format!("shader_{:04X}.glsl", kind as u32);

    compiler
        .compile_into_spirv(source, kind, &file_name, "main", Some(&options))
        .map_err(|e| {
            log::debug!("[ShaderTranslator] shaderc compile failed for {:?}: {}", env, e);
        })
        .ok()
        .map(|a| a.as_binary().to_vec())
}

fn shader_kind(stage: u32) -> shaderc::ShaderKind {
    match stage {
        GL_VERTEX_SHADER => shaderc::ShaderKind::Vertex,
        GL_FRAGMENT_SHADER => shaderc::ShaderKind::Fragment,
        GL_GEOMETRY_SHADER => shaderc::ShaderKind::Geometry,
        GL_TESS_CONTROL_SHADER => shaderc::ShaderKind::TessControl,
        GL_TESS_EVALUATION_SHADER => shaderc::ShaderKind::TessEvaluation,
        GL_COMPUTE_SHADER => shaderc::ShaderKind::Compute,
        _ => shaderc::ShaderKind::InferFromSource,
    }
}

fn spirv_to_gles(spv: &[u32], stage: u32, source: &str) -> Option<String> {
    let module = naga::front::spv::parse_u8_slice(
        bytemuck::cast_slice(spv),
        &SpvOptions {
            adjust_coordinate_space: false,
            strict_capabilities: false,
            block_ctx_dump_prefix: None,
        },
    )
    .map_err(|e| {
        log::warn!("[ShaderTranslator] naga spirv-in failed: {:?}", e);
    })
    .ok()?;

    let capabilities = build_capabilities();
    let info = naga::valid::Validator::new(ValidationFlags::all(), capabilities)
        .validate(&module)
        .map_err(|e| {
            log::warn!(
                "[ShaderTranslator] naga validation failed (capabilities={:?}): {:?}",
                capabilities, e
            );
        })
        .ok()?;

    let glsl_stage = naga_stage(stage)?;
    let version = gles_target_version(source);
    let options = GlslOptions {
        version: naga::back::glsl::Version::Embedded {
            version,
            is_webgl: false,
        },
        writer_flags: WriterFlags::empty(),
        binding_map: Default::default(),
        zero_initialize_workgroup_memory: true,
    };
    let pipeline_options = PipelineOptions {
        shader_stage: glsl_stage,
        entry_point: "main".to_string(),
        multiview: None,
    };

    let mut output = String::new();
    let mut writer = naga::back::glsl::Writer::new(
        &mut output,
        &module,
        &info,
        &options,
        &pipeline_options,
        naga::proc::BoundsCheckPolicies::default(),
    )
    .map_err(|e| {
        log::warn!("[ShaderTranslator] naga glsl writer creation failed: {:?}", e);
    })
    .ok()?;

    writer
        .write()
        .map_err(|e| {
            log::warn!("[ShaderTranslator] naga glsl write failed: {:?}", e);
        })
        .ok()?;

    Some(output)
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

fn gles_target_version(source: &str) -> u16 {
    // Prefer GLES 3.0 for the broadest compatibility. Only use 3.2 when the
    // source explicitly requested a high desktop version or uses 3.2-only
    // features (e.g. textureGather).
    if source.contains("#version 460")
        || source.contains("#version 450")
        || source.contains("textureGather")
    {
        320
    } else {
        300
    }
}

fn naga_stage(stage: u32) -> Option<naga::ShaderStage> {
    match stage {
        GL_VERTEX_SHADER => Some(naga::ShaderStage::Vertex),
        GL_FRAGMENT_SHADER => Some(naga::ShaderStage::Fragment),
        GL_COMPUTE_SHADER => Some(naga::ShaderStage::Compute),
        // naga 30 does not model geometry/tessellation stages in its IR, so
        // SPIR-V containing them will fail here and fall back to the string pass.
        GL_GEOMETRY_SHADER | GL_TESS_CONTROL_SHADER | GL_TESS_EVALUATION_SHADER => {
            log::debug!(
                "[ShaderTranslator] stage 0x{:04X} is not modelled by naga 30, falling back",
                stage
            );
            None
        }
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
