use glslang::{
    Compiler, CompilerOptions, OpenGlVersion, ShaderInput, ShaderMessage, ShaderSource,
    ShaderStage, SourceLanguage, SpirvVersion, Target,
};
use regex::Regex;
use spirv_cross2::compile::glsl::GlslVersion;
use spirv_cross2::compile::{CompilableTarget, CompiledArtifact};
use spirv_cross2::targets::Glsl;
use spirv_cross2::{Compiler as SpvCompiler, Module, SpirvCrossError};

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
    log::debug!(
        "[ShaderTranslator] SPIR-V translate start: stage={} (0x{:04X})",
        stage_name,
        stage
    );

    let spv = match compile_to_spirv(source, stage) {
        Some(s) => s,
        None => {
            return TranslationResult::Failed;
        }
    };

    for gles_version in gles_version_candidates(source) {
        match spirv_to_gles(&spv, gles_version) {
            Ok(src) => {
                log::debug!(
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
    // glslang 全局编译器需要先初始化，OnceLock 保证只会初始化一次
    initialize_glslang()?;
    let compiler = Compiler::acquire()?;
    let glsl_stage = map_gl_stage(stage)?;
    let src = ShaderSource::from(source);

    let options = CompilerOptions {
        source_language: SourceLanguage::GLSL,
        target: Target::OpenGL {
            version: OpenGlVersion::OpenGL4_5,
            spirv_version: Some(SpirvVersion::SPIRV1_0),
        },
        version_profile: None,
        messages: ShaderMessage::SUPPRESS_WARNINGS,
    };

    let input = match ShaderInput::new(
        &src,
        glsl_stage,
        &options,
        None::<&[(&str, Option<&str>)]>,
        None,
    ) {
        Ok(input) => input,
        Err(e) => {
            log::error!(
                "[ShaderTranslator] glslang parse failed for stage 0x{:04X}: {:?}",
                stage,
                e
            );
            return None;
        }
    };

    let shader = match compiler.create_shader(input) {
        Ok(shader) => shader,
        Err(e) => {
            log::error!(
                "[ShaderTranslator] glslang shader creation failed for stage 0x{:04X}: {:?}",
                stage,
                e
            );
            return None;
        }
    };

    match shader.compile() {
        Ok(spv) => Some(spv),
        Err(e) => {
            log::error!(
                "[ShaderTranslator] glslang SPIR-V compile failed for stage 0x{:04X}: {:?}",
                stage,
                e
            );
            None
        }
    }
}

/// 初始化 glslang 全局编译器（线程安全，仅首次调用时执行初始化）
/// 注意：不能依赖 `Compiler::acquire()` 做初始化，因为它在失败时会将 `None` 永久缓存到 OnceLock 中
fn initialize_glslang() -> Option<()> {
    use std::sync::OnceLock;
    static INIT: OnceLock<bool> = OnceLock::new();
    let ok = INIT.get_or_init(|| {
        let result = unsafe { glslang_sys::glslang_initialize_process() != 0 };
        if !result {
            log::error!("[ShaderTranslator] glslang_initialize_process() failed!");
        } else {
            log::debug!("[ShaderTranslator] glslang initialized successfully");
        }
        result
    });
    if *ok { Some(()) } else { None }
}

fn map_gl_stage(stage: u32) -> Option<ShaderStage> {
    match stage {
        GL_VERTEX_SHADER => Some(ShaderStage::Vertex),
        GL_FRAGMENT_SHADER => Some(ShaderStage::Fragment),
        GL_GEOMETRY_SHADER => Some(ShaderStage::Geometry),
        GL_TESS_CONTROL_SHADER => Some(ShaderStage::TesselationControl),
        GL_TESS_EVALUATION_SHADER => Some(ShaderStage::TesselationEvaluation),
        GL_COMPUTE_SHADER => Some(ShaderStage::Compute),
        _ => None,
    }
}

fn spirv_to_gles(spv: &[u32], version: u16) -> Result<String, SpirvCrossError> {
    let module = Module::from_words(spv);
    let compiler = SpvCompiler::<Glsl>::new(module)?;
    let mut options = Glsl::options();

    options.version = match version {
        320 => GlslVersion::Glsl320Es,
        310 => GlslVersion::Glsl310Es,
        300 => GlslVersion::Glsl300Es,
        _ => GlslVersion::Glsl300Es,
    };

    let artifact: CompiledArtifact<Glsl> = compiler.compile(&options)?;
    let src = artifact.to_string();

    Ok(post_process_glsl_es(&src))
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

fn gles_version_candidates(source: &str) -> Vec<u16> {
    let desktop_version = extract_version(source)
        .and_then(|line| {
            line.split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<u32>().ok())
        })
        .unwrap_or(150);
    match desktop_version {
        460 | 450 | 440 | 430 | 420 | 410 | 400 | 330 => vec![320, 310, 300],
        _ => vec![310, 300],
    }
}

/// 后处理 GLSL ES 代码，移除会导致链接冲突的硬编码 layout 修饰符
fn post_process_glsl_es(src: &str) -> String {
    let mut result = src.to_string();

    // 1. 移除 Sampler 和 Image 的 layout
    let re_sampler = Regex::new(
        r"(?i)layout\s*\([^)]*\)\s*(uniform\s+(?:highp\s+|mediump\s+|lowp\s+)?(?:sampler|image))",
    )
    .unwrap();
    result = re_sampler.replace_all(&result, "$1").to_string();

    // 2. 移除 Uniform Block (UBO) 的 binding = X
    let re_ubo = Regex::new(r"(?i)layout\s*\(([^)]*)\)\s*(uniform\s+[A-Za-z0-9_]+\s*\{)").unwrap();
    let re_binding = Regex::new(r"(?i),?\s*binding\s*=\s*\d+\s*,?").unwrap();
    let re_multi_comma = Regex::new(r",\s*,").unwrap();

    result = re_ubo
        .replace_all(&result, |caps: &regex::Captures| {
            let mut params = caps[1].to_string();
            params = re_binding.replace_all(&params, "").to_string();
            params = params
                .trim_matches(|c: char| c == ',' || c.is_whitespace())
                .to_string();
            params = re_multi_comma.replace_all(&params, ",").to_string();
            if params.is_empty() {
                format!("{}", &caps[2])
            } else {
                format!("layout({}) {}", params, &caps[2])
            }
        })
        .to_string();

    // 3. 移除 in/out 变量的 location，解决 VS/FS 链接时的 Input Output Mismatch
    let re_io = Regex::new(r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*\)\s*(in|out)\b").unwrap();
    result = re_io.replace_all(&result, "$1").to_string();

    // 4. 移除非 opaque 普通 uniform 上的 layout 限定符
    let re_uniform = Regex::new(
        r"(?i)layout\s*\(\s*(?:location\s*=\s*\d+\s*,?\s*|binding\s*=\s*\d+\s*,?\s*)+\)\s*(uniform\s+[^;{]+;)",
    )
    .unwrap();
    result = re_uniform.replace_all(&result, "$1").to_string();

    result
}