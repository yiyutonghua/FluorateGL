use regex::Regex;
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
    log::debug!(
        "[ShaderTranslator] SPIR-V translate start: stage={} (0x{:04X})",
        stage_name,
        stage
    );

    let spv = match compile_to_spirv(source, stage) {
        Some(s) => s,
        None => {
            // 这里不需要再 log 了，compile_to_spirv 内部已经打印了详细错误
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
    let compiler = shaderc::Compiler::new().ok()?;
    let mut options = shaderc::CompileOptions::new().ok()?;

    options.set_target_env(
        shaderc::TargetEnv::OpenGL,
        shaderc::EnvVersion::OpenGL4_5 as u32,
    );
    options.set_source_language(shaderc::SourceLanguage::GLSL);

    options.set_optimization_level(shaderc::OptimizationLevel::Zero);
    options.set_auto_map_locations(true);
    options.set_auto_bind_uniforms(true);
    options.set_suppress_warnings();

    let kind = shader_kind(stage);
    let file_name = format!("shader_{:04X}.glsl", kind as u32);

    compiler
        .compile_into_spirv(source, kind, &file_name, "main", Some(&options))
        .map_err(|e| {
            log::error!("[ShaderTranslator] shaderc compile failed: {}", e);
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
    let src = artifact.to_string();

    // ✅ 核心修复：调用后处理函数，解决 Link 阶段的所有冲突
    Ok(post_process_glsl_es(&src))
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

    // ✅ 3. 核心修复：移除 in/out 变量的 location，解决 VS/FS 链接时的 Input Output Mismatch
    // 匹配: layout(location = 0) in vec4 texCoord0; -> in vec4 texCoord0;
    let re_io = Regex::new(r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*\)\s*(in|out)\b").unwrap();
    result = re_io.replace_all(&result, "$1").to_string();

    // ✅ 4. 修复：移除非 opaque 普通 uniform（如 mat4/vec3/float）上的 layout 限定符。
    // spirv-cross 会给所有 uniform 加 layout(location=.., binding=..)，但 GLES 只允许
    // 在 UBO / storage block / opaque 变量上使用 binding，否则报：
    //   "the binding qualifier only applies to uniform blocks, storage blocks, opaque variables..."
    // 仅匹配以 `;` 结尾且不含 `{` 的普通 uniform 声明（UBO block 以 `{` 开头，自然排除）。
    // 注：Rust regex crate 不支持 lookahead，故用 `[^;{]+` 显式限定。
    let re_uniform = Regex::new(
        r"(?i)layout\s*\(\s*(?:location\s*=\s*\d+\s*,?\s*|binding\s*=\s*\d+\s*,?\s*)+\)\s*(uniform\s+[^;{]+;)",
    )
    .unwrap();
    result = re_uniform.replace_all(&result, "$1").to_string();

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_version_finds_version_line() {
        let src = "#version 330 core\nvoid main() {}\n";
        assert_eq!(extract_version(src), Some("#version 330 core"));
    }

    #[test]
    fn extract_version_skips_leading_whitespace_and_comments() {
        let src = "  #version 460 core\nvoid main() {}\n";
        assert!(extract_version(src).unwrap().contains("#version 460"));
    }

    #[test]
    fn extract_version_returns_none_when_absent() {
        assert_eq!(extract_version("void main() {}\n"), None);
    }

    #[test]
    fn gles_version_candidates_for_330_plus_yields_320_first() {
        let src = "#version 460 core\nvoid main() {}\n";
        let v = gles_version_candidates(src);
        assert_eq!(v, vec![320, 310, 300]);
    }

    #[test]
    fn gles_version_candidates_for_legacy_yields_310_first() {
        let src = "#version 150 core\nvoid main() {}\n";
        let v = gles_version_candidates(src);
        assert_eq!(v, vec![310, 300]);
    }

    #[test]
    fn gles_version_candidates_defaults_to_legacy_when_no_version() {
        let v = gles_version_candidates("void main() {}\n");
        assert_eq!(v, vec![310, 300]);
    }

    /// 测试 post_process_glsl_es 移除普通 uniform 的 layout 限定符。
    /// 这是测试驱动的修复：spirv-cross 给 `uniform mat4` 加了
    /// `layout(location=0, binding=0)`，GLES 不接受。
    #[test]
    fn post_process_strips_layout_from_plain_uniform() {
        let src = "#version 320 es\nlayout(location = 0, binding = 0) uniform mat4 MVP;\nvoid main() {}\n";
        let out = post_process_glsl_es(src);
        assert!(
            out.contains("uniform mat4 MVP;"),
            "expected plain uniform, got: {}",
            out
        );
        assert!(!out.contains("binding = 0) uniform mat4"));
    }

    /// 测试 post_process_glsl_es 移除 in/out 变量的 location 限定符。
    #[test]
    fn post_process_strips_location_from_in_out() {
        let src = "#version 320 es\nlayout(location = 0) in vec3 Position;\nlayout(location = 0) out vec2 vUV;\nvoid main() {}\n";
        let out = post_process_glsl_es(src);
        assert!(out.contains("in vec3 Position;"), "got: {}", out);
        assert!(out.contains("out vec2 vUV;"), "got: {}", out);
    }

    /// 测试 post_process_glsl_es 移除 sampler 的 layout 限定符。
    #[test]
    fn post_process_strips_layout_from_sampler() {
        let src = "#version 320 es\nlayout(binding = 0) uniform sampler2D tex;\nvoid main() {}\n";
        let out = post_process_glsl_es(src);
        assert!(out.contains("uniform sampler2D tex;"), "got: {}", out);
    }

    /// 测试 post_process_glsl_es 保留 UBO 的 std140 layout（仅清理 binding）。
    #[test]
    fn post_process_keeps_ubo_std140_layout() {
        let src = "#version 320 es\nlayout(std140, binding = 0) uniform Block { mat4 m; };\nvoid main() {}\n";
        let out = post_process_glsl_es(src);
        // UBO 应保留 std140，仅移除 binding
        assert!(out.contains("layout(std140)"), "got: {}", out);
        assert!(!out.contains("binding = 0) uniform Block"), "got: {}", out);
    }

    /// 测试完整 translate 流程：桌面 GLSL vertex shader → GLSL ES。
    /// 这是端到端测试，依赖 shaderc（已静态链接进库）和 spirv-cross。
    /// 注：translate 内部的 log 调用在未设置全局 logger 时是 no-op，不影响测试结果。
    #[test]
    fn translate_vertex_shader_330_to_gles_es() {
        let src = "#version 330 core\nuniform mat4 MVP;\nin vec3 Position;\nvoid main() { gl_Position = MVP * vec4(Position, 1.0); }\n";
        let result = translate(src, GL_VERTEX_SHADER);
        match result {
            TranslationResult::Translated(out) => {
                assert!(out.contains("#version"), "missing #version: {}", out);
                assert!(out.contains("uniform mat4 MVP"), "plain uniform kept layout: {}", out);
                assert!(!out.contains("binding = 0) uniform mat4"), "got: {}", out);
            }
            other => panic!("expected Translated, got {:?} (shaderc/spirv-cross available?)", other),
        }
    }

    /// 测试完整 translate 流程：fragment shader + sampler2D。
    #[test]
    fn translate_fragment_shader_with_sampler() {
        let src = "#version 330 core\nuniform sampler2D tex;\nin vec2 uv;\nout vec4 fragColor;\nvoid main() { fragColor = texture(tex, uv); }\n";
        let result = translate(src, GL_FRAGMENT_SHADER);
        match result {
            TranslationResult::Translated(out) => {
                // spirv-cross 会注入精度限定符（如 highp），故用更宽松的匹配
                assert!(out.contains("sampler2D tex"), "got: {}", out);
                assert!(out.contains("texture(tex, uv)"), "got: {}", out);
            }
            other => panic!("expected Translated, got {:?}", other),
        }
    }

    /// 测试无效源码不会 panic，返回 Failed。
    #[test]
    fn translate_invalid_shader_does_not_panic() {
        let src = "#version 330 core\nthis is not valid glsl\n";
        let result = translate(src, GL_VERTEX_SHADER);
        assert!(matches!(result, TranslationResult::Failed), "expected Failed, got {:?}", result);
    }

    #[test]
    fn stage_name_covers_all_stages() {
        assert_eq!(stage_name(GL_VERTEX_SHADER), "vertex");
        assert_eq!(stage_name(GL_FRAGMENT_SHADER), "fragment");
        assert_eq!(stage_name(GL_GEOMETRY_SHADER), "geometry");
        assert_eq!(stage_name(GL_TESS_CONTROL_SHADER), "tess_control");
        assert_eq!(stage_name(GL_TESS_EVALUATION_SHADER), "tess_eval");
        assert_eq!(stage_name(GL_COMPUTE_SHADER), "compute");
        assert_eq!(stage_name(0xDEAD_BEEF), "unknown");
    }
}
