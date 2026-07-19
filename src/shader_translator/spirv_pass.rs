use spirv_cross2::compile::glsl::GlslVersion;
use spirv_cross2::compile::{CompilableTarget, CompiledArtifact};
use spirv_cross2::targets::Glsl;
use spirv_cross2::{Compiler, Module, SpirvCrossError};

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;
const GL_COMPUTE_SHADER: u32 = 0x91B9;

#[derive(Debug, Clone)]
pub enum TranslationResult {
    Translated(String),
    PassThrough,
    Failed,
}

pub fn translate(source: &str, stage: u32) -> TranslationResult {
    match std::panic::catch_unwind(|| translate_internal(source, stage)) {
        Ok(result) => result,
        Err(_) => {
            log::error!(
                "[ShaderTranslator] SPIR-V translation panicked for stage 0x{:04X}; skipping",
                stage
            );
            TranslationResult::Failed
        }
    }
}

fn translate_internal(source: &str, stage: u32) -> TranslationResult {
    let stage_name = stage_name(stage);
    log::info!(
        "[ShaderTranslator] SPIR-V translate start: stage={} (0x{:04X})",
        stage_name,
        stage
    );

    let spv = match compile_to_spirv(source, stage) {
        Some(s) => s,
        None => {
            log::warn!(
                "[ShaderTranslator] shaderc compile failed for stage {}",
                stage_name
            );
            return TranslationResult::Failed;
        }
    };

    for gles_version in gles_version_candidates(source) {
        match spirv_to_gles(&spv, gles_version) {
            Ok(src) => {
                log::info!(
                    "[ShaderTranslator] SPIR-V translate success: stage={}, version=ES{}",
                    stage_name,
                    gles_version
                );
                log::debug!("[ShaderTranslator] translated GLSL ES:\n{}", src);
                return TranslationResult::Translated(src);
            }
            Err(e) => {
                log::warn!(
                    "[ShaderTranslator] GLES ES{} write failed for stage {}: {:?}",
                    gles_version,
                    stage_name,
                    e
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

fn compile_to_spirv(source: &str, stage: u32) -> Option<Vec<u32>> {
    let compiler = shaderc::Compiler::new().ok()?;
    let mut options = shaderc::CompileOptions::new().ok()?;

    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_1 as u32,
    );
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);
    options.set_auto_map_locations(true);
    options.set_auto_bind_uniforms(true);
    options.set_suppress_warnings();

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

fn spirv_to_gles(spv: &[u32], version: u16) -> Result<String, SpirvCrossError> {
    let module = Module::from_words(spv);
    let compiler = Compiler::<Glsl>::new(module)?;
    let mut options = Glsl::options();

    options.version = match version {
        300 => GlslVersion::Glsl300Es,
        310 => GlslVersion::Glsl310Es,
        320 => GlslVersion::Glsl320Es,
        _ => GlslVersion::Glsl300Es,
    };

    let artifact: CompiledArtifact<Glsl> = compiler.compile(&options)?;
    Ok(artifact.to_string())
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

fn stage_name(stage: u32) -> &'static str {
    match stage {
        GL_VERTEX_SHADER => "vertex ",
        GL_FRAGMENT_SHADER => "fragment ",
        GL_GEOMETRY_SHADER => "geometry ",
        GL_TESS_CONTROL_SHADER => "tess_control ",
        GL_TESS_EVALUATION_SHADER => "tess_eval ",
        GL_COMPUTE_SHADER => "compute ",
        _ => "unknown ",
    }
}

fn extract_version(source: &str) -> Option<&str> {
    source
        .lines()
        .find(|l| l.trim_start().starts_with("#version"))
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
