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
/// Returns `None` when the input uses unsupported shader stages
/// (geometry/tessellation) or when any pipeline step fails.
pub fn translate(source: &str, stage: u32) -> Option<String> {
    let spv = compile_to_spirv(source, stage)?;
    spirv_to_gles(&spv, stage)
}

fn compile_to_spirv(source: &str, stage: u32) -> Option<Vec<u32>> {
    let compiler = shaderc::Compiler::new().ok()?;
    let mut options = shaderc::CompileOptions::new().ok()?;

    // Target OpenGL semantics so the generated SPIR-V is closer to what
    // naga's spirv-in expects for desktop GLSL.
    options.set_target_env(shaderc::TargetEnv::OpenGL, shaderc::EnvVersion::OpenGL4_5 as u32);
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);
    options.set_generate_debug_info();

    let kind = shader_kind(stage);
    let file_name = format!("shader_{:04X}.glsl", stage);

    let artifact = compiler
        .compile_into_spirv(source, kind, &file_name, "main", Some(&options))
        .map_err(|e| {
            log::warn!("[ShaderTranslator] shaderc compile failed: {}", e);
        })
        .ok()?;

    Some(artifact.as_binary().to_vec())
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

fn spirv_to_gles(spv: &[u32], stage: u32) -> Option<String> {
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

    let info = naga::valid::Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| {
            log::warn!("[ShaderTranslator] naga validation failed: {:?}", e);
        })
        .ok()?;

    let glsl_stage = naga_stage(stage)?;
    let options = GlslOptions {
        version: naga::back::glsl::Version::Embedded {
            version: 320,
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

fn naga_stage(stage: u32) -> Option<naga::ShaderStage> {
    match stage {
        GL_VERTEX_SHADER => Some(naga::ShaderStage::Vertex),
        GL_FRAGMENT_SHADER => Some(naga::ShaderStage::Fragment),
        GL_COMPUTE_SHADER => Some(naga::ShaderStage::Compute),
        GL_GEOMETRY_SHADER | GL_TESS_CONTROL_SHADER | GL_TESS_EVALUATION_SHADER => {
            log::warn!(
                "[ShaderTranslator] unsupported shader stage 0x{:04X} for SPIR-V pipeline",
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
