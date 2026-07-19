// spirv_pass.rs
// 使用 shaderc + spirv-cross2 将桌面 GLSL 转换为 GLSL ES
// 适用于 Rust 2024 edition

// 核心类型
use spirv_cross2::{Compiler, Module, SpirvCrossError};
// 编译相关（GLSL 后端）
use spirv_cross2::compile::{CompilableTarget, CompiledArtifact};
use spirv_cross2::compile::glsl::GlslVersion;
// 目标后端
use spirv_cross2::targets::Glsl;

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
/// spirv-cross (SPIR-V -> GLSL ES).
///
/// All stages are attempted, including geometry and tessellation.
/// If the translation succeeds, the result is `Translated`; otherwise `Failed`.
/// The `PassThrough` variant is currently never returned (but kept for API compatibility).
pub fn translate(source: &str, stage: u32) -> TranslationResult {
    // spirv-cross is generally stable, but keep catch_unwind as a safety net.
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
        stage_name, stage
    );

    // 1. Compile desktop GLSL to SPIR-V
    let spv = match compile_to_spirv(source, stage) {
        Some(s) => s,
        None => {
            log::warn!("[ShaderTranslator] shaderc compile failed for stage {}", stage_name);
            return TranslationResult::Failed;
        }
    };

    // 2. Try each GLES version candidate in order (prefer most capable first)
    for gles_version in gles_version_candidates(source) {
        match spirv_to_gles(&spv, gles_version) {
            Ok(src) => {
                log::info!(
                    "[ShaderTranslator] SPIR-V translate success: stage={}, version=ES{}",
                    stage_name, gles_version
                );
                log::debug!("[ShaderTranslator] translated GLSL ES:\n{}", src);
                return TranslationResult::Translated(src);
            }
            Err(e) => {
                log::warn!(
                    "[ShaderTranslator] GLES ES{} write failed for stage {}: {:?}",
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

/// Compile GLSL source to SPIR-V using shaderc.
/// No preprocessing (combined sampler splitting, uniform block wrapping) is needed
/// because spirv-cross handles everything natively.
fn compile_to_spirv(source: &str, stage: u32) -> Option<Vec<u32>> {
    let compiler = shaderc::Compiler::new().ok()?;
    let mut options = shaderc::CompileOptions::new().ok()?;

    // Always use Vulkan semantics: spirv-cross expects Vulkan-style SPIR-V.
    options.set_target_env(shaderc::TargetEnv::Vulkan, shaderc::EnvVersion::Vulkan1_0 as u32);
    // Keep optimizations off to avoid potential issues with spirv-cross.
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);

    // Auto-generate locations and bindings (required for Vulkan).
    options.set_auto_map_locations(true);
    options.set_auto_bind_uniforms(true);
    options.set_suppress_warnings();

    let kind = shader_kind(stage);
    let file_name = format!("shader_{:04X}.glsl", kind as u32);

    // Directly compile the original source – no preprocessing.
    compiler
        .compile_into_spirv(source, kind, &file_name, "main", Some(&options))
        .map_err(|e| {
            log::warn!("[ShaderTranslator] shaderc compile failed: {}", e);
        })
        .ok()
        .map(|a| a.as_binary().to_vec())
}

fn spirv_to_gles(spv: &[u32], version: u16) -> Result<String, SpirvCrossError> {
    let module = Module::from_words(spv);              // 这里没有 ?，因为返回 Module
    let compiler = Compiler::<Glsl>::new(module)?;      // 去掉 &，传递所有权
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

/// Map GL constant to shaderc shader kind.
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

/// Return human-readable stage name.
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

/// Extract #version line from source to guess the desktop GLSL version.
fn extract_version(source: &str) -> Option<&str> {
    source
        .lines()
        .find(|l| l.trim_start().starts_with("#version"))
}

/// Determine which GLES versions to attempt, based on the input desktop version.
/// Returns candidates from most capable to least capable.
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