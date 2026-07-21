use glslang::{
    Compiler, CompilerOptions, ShaderInput, ShaderMessage, ShaderOptions, ShaderSource,
    ShaderStage, SourceLanguage, SpirvVersion, Target, VulkanVersion,
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
            log::warn!(
                "[ShaderTranslator] glslang SPIR-V compile failed for stage {} (0x{:04X})",
                stage_name,
                stage
            );
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
    let compiler = match Compiler::acquire() {
        Some(c) => c,
        None => {
            log::error!(
                "[ShaderTranslator] glslang compiler not available (glslang_initialize_process failed)"
            );
            return None;
        }
    };
    let glsl_stage = map_gl_stage(stage)?;

    // 预处理 GLSL：对齐 MobileGlues 的 preprocess_glsl + get_or_add_glsl_version
    // - 移除 #line 指令
    // - 强制 GLSL 版本 >= 150（兼容 MobileGlues 行为）
    let preprocessed = preprocess_glsl_source(source);

    let src = ShaderSource::from(preprocessed.as_str());

    // 对齐 MobileGlues: Target::Vulkan（EShClientVulkan 输入）
    // 使用 Vulkan 目标解析 GLSL 更宽松（要求 GLSL >= 140 而非 330）
    let options = CompilerOptions {
        source_language: SourceLanguage::GLSL,
        target: Target::Vulkan {
            version: VulkanVersion::Vulkan1_0,
            spirv_version: SpirvVersion::SPIRV1_5,
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

    let mut shader = match compiler.create_shader(input) {
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

    // 对齐 MobileGlues: 开启 AutoMapBindings + AutoMapLocations + VulkanRulesRelaxed
    shader.options(
        ShaderOptions::AUTO_MAP_BINDINGS
            | ShaderOptions::AUTO_MAP_LOCATIONS
            | ShaderOptions::VULKAN_RULES_RELAXED,
    );

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

/// 对齐 MobileGlues preprocess_glsl + get_or_add_glsl_version
/// 1. 移除 #line 指令
/// 2. 强制 GLSL 版本 >= 150（无版本则插入 #version 150）
fn preprocess_glsl_source(source: &str) -> String {
    let mut result = remove_line_directives(source);

    let version = extract_version(&result);
    match version {
        None => {
            // 没有 version 指令，插入 #version 150
            result.insert_str(0, "#version 150\n");
        }
        Some(v) => {
            if let Ok(ver) = v.parse::<u32>() {
                if ver < 140 {
                    // 旧版本强制升级到 150 compatibility
                    let re = Regex::new(r"(?m)^#version\s+\d+.*$").unwrap();
                    result = re
                        .replace(&result, "#version 150 compatibility")
                        .to_string();
                }
            }
        }
    }

    result
}

/// 移除 #line 指令（对齐 MobileGlues replace_line_starting_with("#line")）
fn remove_line_directives(source: &str) -> String {
    let re = Regex::new(r"(?m)^\s*#line\s+.*$(\n|$)?").unwrap();
    re.replace_all(source, "").to_string()
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

    // spirv-cross2 默认 es_default_float_precision_highp = false（mediump），
    // MC shader 需要 highp 避免精度不足
    options.es_default_float_precision_highp = true;
    options.es_default_int_precision_highp = true;

    // GLES 坐标系 Y 轴翻转（对齐 MobileGlues）
    options.common.flip_vertex_y = true;

    // Vulkan [0,w] 深度范围 → OpenGL [-w,w] 深度范围（对齐 MobileGlues）
    options.common.fixup_clipspace = true;

    // 不输出 #line 指令（我们在后处理中统一清理）
    options.common.emit_line_directives = false;

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

/// 后处理 GLSL ES 代码，对齐 MobileGlues 的 removeLayoutBinding + processOutColorLocations + forceSupporterOutput
fn post_process_glsl_es(src: &str) -> String {
    let mut result = src.to_string();

    // 1. 移除所有 layout(binding = X)（对齐 MobileGlues removeLayoutBinding）
    //    MobileGlues 不分类型，全部移除。需要处理以下形式：
    //    - layout(binding = X)                            → 移除整个 layout(...)
    //    - layout(binding = X, std140)                    → layout(std140)
    //    - layout(std140, binding = X)                    → layout(std140)
    //    - layout(std140, binding = X, column_major)      → layout(std140, column_major)
    //    - layout(push_constant, binding = X)             → layout(push_constant)
    //    策略：先处理 binding=X 在中间/末尾的情况，再处理 binding=X 是唯一项的情况
    let re_binding = Regex::new(r"(?i)layout\s*\(\s*binding\s*=\s*\d+\s*\)\s*").unwrap();
    let re_binding_leading = Regex::new(r"(?i)layout\s*\(\s*binding\s*=\s*\d+\s*,\s*").unwrap();
    let re_binding_middle = Regex::new(r"(?i),\s*binding\s*=\s*\d+").unwrap();

    // 先处理 binding 在中间的情况: layout(..., binding=X, ...) → layout(..., ...)
    result = re_binding_middle.replace_all(&result, "").to_string();
    // 再处理 binding 在开头的情况: layout(binding=X, ...) → layout(...)
    result = re_binding_leading.replace_all(&result, "layout(").to_string();
    // 最后处理 binding 是唯一项的情况: layout(binding=X) → 移除
    result = re_binding.replace_all(&result, "").to_string();

    // 清理可能残留的空 layout 括号
    let re_empty_layout = Regex::new(r"(?i)layout\s*\(\s*\)\s*").unwrap();
    result = re_empty_layout.replace_all(&result, "").to_string();

    // 2. 处理 outColorN 的 location（对齐 MobileGlues processOutColorLocations）
    let re_out_color =
        Regex::new(r"(?m)^(out\s+(?:highp\s+|mediump\s+|lowp\s+)?\w+\s+outColor)(\d+)\s*;")
            .unwrap();
    result = re_out_color
        .replace_all(&result, "layout(location=$2) $1$2;")
        .to_string();

    // 3. 确保 precision 声明（对齐 MobileGlues forceSupporterOutput）
    result = ensure_precision(&result);

    result
}

/// 确保 precision highp float/int 声明存在（对齐 MobileGlues forceSupporterOutput）
/// 始终强制使用 highp，移除所有已有 precision 声明后统一插入
fn ensure_precision(source: &str) -> String {
    let mut result = source.to_string();

    // 移除所有已有的 precision 声明（注释中的不受影响，因为 #version 之后不会有注释行）
    let re_precision =
        Regex::new(r"(?m)^\s*precision\s+\w+\s+(?:float|int)\s*;.*$(\n)?").unwrap();
    result = re_precision.replace_all(&result, "").to_string();

    let precision_decl = "precision highp float;\nprecision highp int;\n";

    // 在 #extension 之后或 #version 之后插入
    let last_ext = result.rfind("#extension");
    if let Some(pos) = last_ext
        .map(|p| result[p..].find('\n').map(|n| p + n + 1))
        .flatten()
    {
        result.insert_str(pos, precision_decl);
    } else if let Some(version_end) = result.find('\n') {
        result.insert_str(version_end + 1, precision_decl);
    } else {
        result.insert_str(0, precision_decl);
    }

    result
}
