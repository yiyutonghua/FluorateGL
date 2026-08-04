//! Spike 验证：重构路线 A —— shaderc `TargetEnv::Vulkan` → `TargetEnv::OpenGL`
//!
//! 背景：当前管线（src/shader_translator/）用 Vulkan target 编译桌面 GLSL
//! （强制 location/binding/UBO 包装），导致 preprocess（1051 行注入 hack）+
//! postprocess（892 行拆除 hack）的大量字符串操作。本测试验证路线 A：
//! glslang OpenGL 语义模式能否直接编译 MC 风格 shader，且 spirv-cross 输出
//! GLES 320 的产物形态是否无需大幅后处理。
//!
//! 第一轮实验结论（无注入直接编译，全部失败，记录于各用例）：
//! - OpenGL target 依然强制 SPIR-V 规则：in/out 需 location、UBO 需 binding、
//!   non-opaque standalone uniform 需 location（`'location' : SPIR-V requires
//!   location for user input/output`）
//! - OpenGL SPIR-V 要求桌面 GLSL >= 330（`#version 150` 被拒）
//! - 但 standalone uniform 合法（只需 location，无需 UBO 包装）
//!
//! 第二轮实验（本文件）：注入 location/binding 后（模拟最小化 preprocess）
//! 验证编译成功与产物形态。
//!
//! 运行：`cargo test --test spike_gl_target_test -- --nocapture`

use fluorategl::shader_translator::{gles_compile, spirv_compile};
use shaderc::{CompileOptions, Compiler, OptimizationLevel, ShaderKind, SpirvVersion};
use spirv_cross2::compile::glsl::GlslVersion;
use spirv_cross2::compile::{CompilableTarget, CompiledArtifact};
use spirv_cross2::targets::Glsl;
use spirv_cross2::{Compiler as SpvCompiler, Module};

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;

// ============ 辅助函数（绕过 src/ 的 preprocess/postprocess） ============

/// 以 `TargetEnv::OpenGL` 直接编译 GLSL（不经过 preprocess）
///
/// `auto_bind`: 是否启用 shaderc 自动 binding 分配（sampler 等 opaque uniform）
fn compile_opengl(
    source: &str,
    stage: u32,
    env_version: u32,
    auto_bind: bool,
) -> Result<Vec<u32>, String> {
    let compiler = Compiler::new().map_err(|e| format!("Compiler::new() failed: {:?}", e))?;
    let mut options =
        CompileOptions::new().map_err(|e| format!("CompileOptions::new() failed: {:?}", e))?;
    options.set_target_env(shaderc::TargetEnv::OpenGL, env_version);
    options.set_optimization_level(OptimizationLevel::Zero);
    options.set_generate_debug_info();
    if auto_bind {
        options.set_auto_bind_uniforms(true);
    }
    let kind = match stage {
        GL_VERTEX_SHADER => ShaderKind::Vertex,
        GL_FRAGMENT_SHADER => ShaderKind::Fragment,
        _ => return Err(format!("unsupported stage 0x{:04X}", stage)),
    };
    let artifact = compiler
        .compile_into_spirv(source, kind, "spike.glsl", "main", Some(&options))
        .map_err(|e| format!("OpenGL env={} compile failed: {:?}", env_version, e))?;
    Ok(artifact.as_binary().to_vec())
}

/// spirv-cross 输出 GLSL ES（与 gles_compile.rs 相同选项，但不经过 postprocess）
fn cross_to_gles(spv: &[u32], es_version: u16) -> Result<String, String> {
    if spv.is_empty() {
        return Err("empty SPIR-V".to_string());
    }
    let module = Module::from_words(spv);
    let compiler = SpvCompiler::<Glsl>::new(module).map_err(|e| format!("SpvCompiler::new: {:?}", e))?;
    let mut options = Glsl::options();
    options.version = match es_version {
        320 => GlslVersion::Glsl320Es,
        310 => GlslVersion::Glsl310Es,
        _ => GlslVersion::Glsl300Es,
    };
    options.es_default_float_precision_highp = true;
    options.es_default_int_precision_highp = true;
    options.common.flip_vertex_y = false;
    options.common.fixup_clipspace = false;
    options.common.emit_line_directives = false;
    let artifact: CompiledArtifact<Glsl> = compiler
        .compile(&options)
        .map_err(|e| format!("spirv-cross compile: {:?}", e))?;
    Ok(artifact.to_string())
}

/// 断言 OpenGL target 编译 + spirv-cross ES320 输出均成功，返回 GLES 产物
fn assert_opengl_pipeline_ok(
    name: &str,
    source: &str,
    stage: u32,
    env: u32,
    auto_bind: bool,
) -> String {
    let spv = compile_opengl(source, stage, env, auto_bind)
        .unwrap_or_else(|e| panic!("[{}] OpenGL target env={} 编译失败: {}", name, env, e));
    println!(
        "[{}] OpenGL env={} 编译成功: SPIR-V {} words (spv[1]=0x{:08X} → SPIR-V {}.{})",
        name,
        env,
        spv.len(),
        spv[1],
        spv[1] >> 16,
        (spv[1] >> 8) & 0xFF
    );
    let es = cross_to_gles(&spv, 320)
        .unwrap_or_else(|e| panic!("[{}] spirv-cross ES320 输出失败: {}", name, e));
    println!("[{}] === GLES 320 产物（无 postprocess） ===\n{}", name, es);
    es
}

/// 检测产物中是否含 UBO/SSBO 实例名（形如 `} _20;` / `} inst;`）
fn has_ubo_instance_name(es: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\}\s*[_A-Za-z][_A-Za-z0-9]*\s*;").unwrap());
    re.is_match(es)
}

// ============ 0. 第一轮失败记录：无注入直接编译（路线 A 的边界） ============

#[test]
fn spike_0_raw_compile_failures_recorded() {
    // 记录：OpenGL target 下无 location/binding 注入的编译失败模式
    // （第二轮起各用例均使用注入后的显式 layout 版本）
    let cases: Vec<(&str, &str, u32, u32)> = vec![
        (
            "a_standalone_uniform",
            "#version 330 core\n\
             uniform mat4 ProjMat;\n\
             in vec3 Position;\n\
             void main() { gl_Position = ProjMat * vec4(Position, 1.0); }\n",
            GL_VERTEX_SHADER,
            330,
        ),
        (
            "b_ubo_no_binding",
            "#version 330 core\n\
             layout(std140) uniform Projection { mat4 ProjMat; };\n\
             in vec3 Position;\n\
             void main() { gl_Position = ProjMat * vec4(Position, 1.0); }\n",
            GL_VERTEX_SHADER,
            330,
        ),
        (
            "c_sampler_no_location",
            "#version 330 core\n\
             in vec2 vUV;\n\
             out vec4 fragColor;\n\
             uniform sampler2D Sampler0;\n\
             void main() { fragColor = texture(Sampler0, vUV); }\n",
            GL_FRAGMENT_SHADER,
            330,
        ),
        (
            "e_150_too_old",
            "#version 150\n\
             attribute vec3 Position;\n\
             varying vec4 vertexColor;\n\
             void main() { gl_Position = vec4(Position, 1.0); vertexColor = vec4(1.0); }\n",
            GL_VERTEX_SHADER,
            330,
        ),
    ];
    for (name, src, stage, env) in cases {
        match compile_opengl(src, stage, env, false) {
            Ok(spv) => println!(
                "[0] 意外成功：{} 无注入编译成功 ({} words)",
                name,
                spv.len()
            ),
            Err(e) => println!("[0] 记录失败（预期）: {} -> {}", name, e),
        }
    }
    println!(
        "[0] 结论：OpenGL target 下 in/out location、UBO binding、non-opaque uniform location 注入仍必需；\
         且桌面 GLSL >= 330（150 被拒）。这些 SPIR-V 规则错误与 Vulkan target 相同。"
    );
}

// ============ 1a. standalone uniform（注入 location 后） ============

#[test]
fn spike_a_standalone_uniform_with_location() {
    // 模拟最小化 preprocess：版本提升 450 + 仅给 uniform/in/out 注入 location，不包装 UBO
    // （330 源码下 layout(location) 对 uniform/buffer 不被支持，需 430+ → 版本提升必需）
    let vs = "#version 450 core\n\
              layout(location=0) uniform mat4 ProjMat;\n\
              layout(location=1) uniform vec4 ColorModulator;\n\
              layout(location=2) uniform vec3 ModelOffset;\n\
              layout(location=0) in vec3 Position;\n\
              layout(location=1) in vec4 Color;\n\
              layout(location=0) out vec4 vertexColor;\n\
              void main() {\n\
                  gl_Position = ProjMat * vec4(Position + ModelOffset, 1.0);\n\
                  vertexColor = Color * ColorModulator;\n\
              }\n";

    for env in [330u32, 450u32] {
        let es = assert_opengl_pipeline_ok("a_standalone+loc", vs, GL_VERTEX_SHADER, env, false);
        // 关键结论：standalone uniform 保留（无需 UBO 包装 / unwrap_generated_ubo）
        assert!(
            es.contains("uniform mat4 ProjMat"),
            "[env={}] ProjMat 应为 standalone uniform 声明，产物:\n{}",
            env,
            es
        );
        assert!(
            es.contains("uniform vec4 ColorModulator"),
            "[env={}] ColorModulator 应为 standalone uniform，产物:\n{}",
            env,
            es
        );
        assert!(
            !es.contains("UniformBlock"),
            "[env={}] 不应有 UniformBlock（unwrap_generated_ubo 可删），产物:\n{}",
            env,
            es
        );
        // 记录：spirv-cross 是否保留 uniform 的 layout(location)（GLES 3.1+ 合法）
        println!(
            "[a] env={} uniform 带 location 输出: {}",
            env,
            es.contains("layout(location") && es.contains("uniform mat4 ProjMat")
        );
        assert!(
            es.contains("gl_Position"),
            "[env={}] 应保留 gl_Position",
            env
        );
    }
}

// ============ 1b. UBO block（注入 binding 后） ============

#[test]
fn spike_b_ubo_block_with_binding() {
    // 330 源码不支持 binding 限定符（需 420+）→ 版本提升 450 后注入 binding
    let vs = "#version 450 core\n\
              layout(std140, binding=0) uniform Projection {\n\
                  mat4 ProjMat;\n\
              };\n\
              layout(std140, binding=1) uniform ColorMod {\n\
                  vec4 ColorModulator;\n\
              };\n\
              layout(location=0) in vec3 Position;\n\
              layout(location=0) out vec4 vertexColor;\n\
              void main() {\n\
                  gl_Position = ProjMat * vec4(Position, 1.0);\n\
                  vertexColor = ColorModulator;\n\
              }\n";

    for env in [330u32, 450u32] {
        let es = assert_opengl_pipeline_ok("b_ubo+binding", vs, GL_VERTEX_SHADER, env, false);
        assert!(
            es.contains("uniform Projection"),
            "[env={}] block 名 Projection 应保留，产物:\n{}",
            env,
            es
        );
        assert!(es.contains("std140"), "[env={}] std140 应保留", env);
        assert!(
            es.contains("ProjMat"),
            "[env={}] 成员 ProjMat 应保留，产物:\n{}",
            env,
            es
        );
        let has_inst = has_ubo_instance_name(&es);
        println!(
            "[b] env={} UBO 实例名检测: {}",
            env,
            if has_inst { "有（strip_ubo_instance_name 仍需保留）" } else { "无（可删）" }
        );
        println!(
            "[b] env={} 结论: {}",
            env,
            if has_inst {
                "spirv-cross 添加了实例名 → strip_ubo_instance_name 仍需保留"
            } else {
                "spirv-cross 未添加实例名 → strip_ubo_instance_name 可删除"
            }
        );
    }
}

// ============ 1c. sampler（无 binding，验证 OpenGL target 不要求） ============

#[test]
fn spike_c_sampler_with_location() {
    // 关键验证：OpenGL target 下 sampler 无需显式 binding（Vulkan 需要 auto_bind）。
    // 源码仅给 in/out 注入 location，sampler 保持裸声明。
    let fs = "#version 450 core\n\
              layout(location=0) in vec2 vUV;\n\
              layout(location=0) out vec4 fragColor;\n\
              uniform sampler2D Sampler0;\n\
              void main() {\n\
                  fragColor = texture(Sampler0, vUV);\n\
              }\n";

    for env in [330u32, 450u32] {
        // auto_bind=false：sampler 无 binding 的源码应编译成功
        let spv = compile_opengl(fs, GL_FRAGMENT_SHADER, env, false)
            .unwrap_or_else(|e| panic!("[c] env={} sampler 无 binding 编译失败: {}", env, e));
        println!(
            "[c] env={} sampler 无 binding 编译成功 ({} words)",
            env,
            spv.len()
        );
        let es = cross_to_gles(&spv, 320).expect("[c] spirv-cross ES320 输出失败");
        println!("[c] env={} === GLES 320 产物 ===\n{}", env, es);
        assert!(
            es.contains("uniform sampler2D Sampler0") || es.contains("uniform highp sampler2D Sampler0"),
            "[env={}] sampler 应保留原名 standalone 声明，产物:\n{}",
            env,
            es
        );
        assert!(
            es.contains("texture(Sampler0, vUV)"),
            "[env={}] texture() 调用应保留",
            env
        );
        // 记录：spirv-cross 对无 binding sampler 自动输出 layout(binding = 0)
        let has_binding = es.contains("binding");
        println!(
            "[c] env={} 产物含 binding: {}（glslang OpenGL 模式自动分配 binding=0；\
             若 300 es 回退/一致性需求，postprocess strip_binding 仍需保留）",
            env, has_binding
        );
    }
}

// ============ 1d. textureQueryLod（GL 4.0 特性，版本差异验证） ============

#[test]
fn spike_d_texture_query_lod_version_diff() {
    // 带 location 注入后，验证 330 vs 450 对 textureQueryLod 的支持差异
    // （330 源码下 location 限定符本身不被支持，且 textureQueryLod 是 GLSL 400+ 内置）
    let fs_330 = "#version 330 core\n\
                  layout(location=0) in vec2 vUV;\n\
                  layout(location=0) out vec4 fragColor;\n\
                  uniform sampler2D Sampler0;\n\
                  void main() {\n\
                      vec2 lod = textureQueryLod(Sampler0, vUV);\n\
                      fragColor = vec4(lod, 0.0, 1.0);\n\
                  }\n";
    let fs_450 = fs_330.replace("#version 330 core", "#version 450 core");

    match compile_opengl(&fs_330, GL_FRAGMENT_SHADER, 330, false) {
        Ok(spv) => println!(
            "[d] OpenGL env=330: textureQueryLod 编译成功 ({} words)",
            spv.len()
        ),
        Err(e) => println!(
            "[d] 记录：OpenGL env=330 textureQueryLod 编译失败（预期，GLSL 330 不支持该内置）-> {}",
            e
        ),
    }
    let spv = compile_opengl(&fs_450, GL_FRAGMENT_SHADER, 450, true)
        .expect("[d] OpenGL env=450 textureQueryLod 应编译成功");
    let es = cross_to_gles(&spv, 320).expect("[d] spirv-cross ES320 输出失败");
    println!("[d] === GLES 320 产物（450 + textureQueryLod） ===\n{}", es);
    // spirv-cross 输出大写的 textureQueryLOD + 自动声明 GL_EXT_texture_query_lod 扩展
    assert!(
        es.contains("textureQueryLOD"),
        "[d] 产物应保留 textureQueryLOD（spirv-cross 大写形式），产物:\n{}",
        es
    );
    assert!(
        es.contains("GL_EXT_texture_query_lod"),
        "[d] 产物应自动声明 GL_EXT_texture_query_lod 扩展，产物:\n{}",
        es
    );
    println!(
        "[d] 结论：textureQueryLod 需版本提升（330 无解，450 成功）。spirv-cross 输出 \
         textureQueryLOD + 扩展声明，若 GLES 3.0 回退/老驱动兼容仍需保留 polyfill，\
         否则 polyfill 可删"
    );
}

// ============ 1e. MC 1.21 风格完整 VS（版本提升 + 注入后） ============

#[test]
fn spike_e_mc_style_full_vs_upgraded() {
    // MC 1.21 core shader 是 #version 150；OpenGL SPIR-V 要求 >= 330。
    // 模拟 preprocess 的 force_glsl_version（150 → 450）+ location/binding 注入。
    let vs = "#version 450 core\n\
              layout(location=0) in vec3 Position;\n\
              layout(location=1) in vec4 Color;\n\
              layout(location=2) in vec2 UV0;\n\
              layout(location=0) out vec2 texCoord0;\n\
              layout(location=1) out vec4 vertexColor;\n\
              layout(location=0) uniform mat4 ModelViewMat;\n\
              layout(location=1) uniform mat4 ProjMat;\n\
              layout(binding=0) uniform sampler2D Sampler2;\n\
              void main() {\n\
                  vec4 worldPos = ModelViewMat * vec4(Position, 1.0);\n\
                  gl_Position = ProjMat * worldPos;\n\
                  texCoord0 = UV0;\n\
                  vertexColor = Color * vec4(float(gl_VertexID), 0.0, 0.0, 1.0);\n\
              }\n";

    for env in [330u32, 450u32] {
        let es = assert_opengl_pipeline_ok("e_mc_vs_upgraded", vs, GL_VERTEX_SHADER, env, false);
        assert!(
            es.contains("gl_Position"),
            "[env={}] 应保留 gl_Position，产物:\n{}",
            env,
            es
        );
        assert!(
            es.contains("vertexColor"),
            "[env={}] varying 应保留",
            env
        );
        assert!(
            es.contains("gl_VertexID"),
            "[env={}] gl_VertexID 应保留原名（OpenGL 语义不重命名为 gl_VertexIndex），产物:\n{}",
            env,
            es
        );
        assert!(
            es.contains("Sampler2"),
            "[env={}] sampler 应保留，产物:\n{}",
            env,
            es
        );
    }

    // 330 core 中 attribute/varying 老语法：仅 deprecated warning，应可编译（记录）
    let vs_330_legacy = "#version 330 core\n\
                         attribute vec3 Position;\n\
                         attribute vec4 Color;\n\
                         varying vec4 vertexColor;\n\
                         void main() {\n\
                             gl_Position = vec4(Position, 1.0);\n\
                             vertexColor = Color * vec4(float(gl_VertexID), 0.0, 0.0, 1.0);\n\
                         }\n";
    match compile_opengl(vs_330_legacy, GL_VERTEX_SHADER, 330, false) {
        Ok(spv) => {
            println!(
                "[e] 记录：330 core + attribute/varying 直接编译成功（仅 deprecated warning，老语法兼容）{} words",
                spv.len()
            );
            let es = cross_to_gles(&spv, 320).unwrap();
            println!("[e] === 330 core 老语法 GLES 320 产物 ===\n{}", es);
        }
        Err(e) => println!("[e] 记录：330 core + attribute/varying 编译失败: {}", e),
    }
}

// ============ 2. OpenGL vs Vulkan 产物对比表 ============

#[test]
fn spike_compare_vulkan_vs_opengl() {
    // 同一 MC 风格 shader：standalone uniform + in/out varying
    // （Vulkan 当前管线输入：原样 330；OpenGL target 输入：版本提升 450 + location 注入）
    let vs = "#version 330 core\n\
              uniform mat4 ProjMat;\n\
              uniform vec4 ColorModulator;\n\
              in vec3 Position;\n\
              in vec4 Color;\n\
              out vec4 vertexColor;\n\
              void main() {\n\
                  gl_Position = ProjMat * vec4(Position, 1.0);\n\
                  vertexColor = Color * ColorModulator;\n\
              }\n";
    // OpenGL target 输入（模拟最小 preprocess：450 + location 注入，无 UBO 包装）
    let vs_gl = "#version 450 core\n\
                 layout(location=0) uniform mat4 ProjMat;\n\
                 layout(location=1) uniform vec4 ColorModulator;\n\
                 layout(location=0) in vec3 Position;\n\
                 layout(location=1) in vec4 Color;\n\
                 layout(location=0) out vec4 vertexColor;\n\
                 void main() {\n\
                     gl_Position = ProjMat * vec4(Position, 1.0);\n\
                     vertexColor = Color * ColorModulator;\n\
                 }\n";

    // 当前管线：Vulkan target（preprocess 注入+包装）→ spirv-cross → postprocess 拆除
    let current_spv = spirv_compile::compile(vs, GL_VERTEX_SHADER)
        .expect("[compare] Vulkan 当前管线编译失败");
    let current_gles =
        gles_compile::compile(&current_spv, 320).expect("[compare] Vulkan 管线 ES320 输出失败");

    // 路线 A：OpenGL target（仅注入，无 UBO 包装、无 postprocess）
    let open_gl_spv = compile_opengl(vs_gl, GL_VERTEX_SHADER, 450, false).unwrap();
    let open_gl_gles = cross_to_gles(&open_gl_spv, 320).unwrap();

    println!("\n======== 对比表：MC 风格 VS（standalone uniform） ========");
    println!("---- 当前 Vulkan target 产物（preprocess + postprocess 后） ----\n{}", current_gles);
    println!("---- OpenGL target 产物（仅 location 注入，无 postprocess） ----\n{}", open_gl_gles);

    let feats = |es: &str| -> Vec<String> {
        vec![
            format!("version: {:?}", es.lines().find(|l| l.starts_with("#version"))),
            format!("standalone uniform: {}", es.contains("uniform mat4 ProjMat;") || es.contains("uniform vec4 ProjMat;")),
            format!("UniformBlock 痕迹: {}", es.contains("UniformBlock")),
            format!("UBO 实例名 (}} _N;): {}", has_ubo_instance_name(es)),
            format!("uniform location: {}", es.contains("uniform mat4 ProjMat") && es.contains("location")),
            format!("in/out location: {}", es.contains("layout(location")),
            format!("binding: {}", es.contains("binding")),
            format!("precision highp float: {}", es.contains("precision highp float")),
            format!("precision highp int: {}", es.contains("precision highp int")),
            format!("gl_Position: {}", es.contains("gl_Position")),
        ]
    };
    let cur = feats(&current_gles);
    let gl = feats(&open_gl_gles);
    println!("---- 特征对比（相同 shader，两种 target） ----");
    for (i, (c, g)) in cur.iter().zip(gl.iter()).enumerate() {
        println!("  [{}] Vulkan : {}", i, c);
        println!("  [{}] OpenGL : {}", i, g);
    }

    // 关键断言：路线 A 产物形态
    assert!(
        open_gl_gles.contains("#version 320 es"),
        "[compare] OpenGL 产物应为 #version 320 es，产物:\n{}",
        open_gl_gles
    );
    assert!(
        open_gl_gles.contains("uniform mat4 ProjMat"),
        "[compare] OpenGL 产物 ProjMat 应为 standalone uniform，产物:\n{}",
        open_gl_gles
    );
    assert!(
        !open_gl_gles.contains("UniformBlock"),
        "[compare] OpenGL 产物不应有 UniformBlock（unwrap_generated_ubo 可删），产物:\n{}",
        open_gl_gles
    );
    // 当前 Vulkan 管线产物对照（postprocess 拆解后的 standalone uniform）
    assert!(
        current_gles.contains("uniform mat4 ProjMat;"),
        "[compare] 当前管线产物应含拆解后的 standalone uniform，产物:\n{}",
        current_gles
    );
}

// ============ 3. 显式 SPIR-V 1.5 在 OpenGL target 下的去留 ============

#[test]
fn spike_opengl_target_with_explicit_spirv_v15() {
    // 现有 spirv_compile.rs 有 `options.set_target_spirv(SpirvVersion::V1_5)`（配合 Vulkan1_2）。
    // 验证 OpenGL target 下显式设置 V1_5 是否被接受（决定重构时该行去留）。
    // 注意：330 源码下 location 限定符不被支持，用 450 源码隔离 SPIR-V 版本变量。
    let src = "#version 450 core\n\
               layout(location=0) uniform mat4 MVP;\n\
               layout(location=0) in vec3 Position;\n\
               void main() { gl_Position = MVP * vec4(Position, 1.0); }\n";
    let compiler = Compiler::new().expect("Compiler::new failed");
    let mut options = CompileOptions::new().expect("CompileOptions::new failed");
    options.set_target_env(shaderc::TargetEnv::OpenGL, 330);
    options.set_target_spirv(SpirvVersion::V1_5);
    options.set_optimization_level(OptimizationLevel::Zero);
    options.set_generate_debug_info();
    match compiler.compile_into_spirv(src, ShaderKind::Vertex, "spike.glsl", "main", Some(&options)) {
        Ok(a) => {
            let spv = a.as_binary().to_vec();
            println!(
                "[spirv15] OpenGL env=330 + 显式 SPIR-V 1.5 编译成功 ({} words, spv[1]=0x{:08X})",
                spv.len(),
                spv[1]
            );
        }
        Err(e) => println!(
            "[spirv15] 记录：OpenGL env=330 + 显式 SPIR-V 1.5 失败: {:?}（重构时需移除 set_target_spirv 行）",
            e
        ),
    }
}

// ============ 4. 补充：150 老语法（attribute/varying）提升 450 后兼容性 ============

#[test]
fn spike_f_legacy_attribute_varying_at_450() {
    // 现状 preprocess 的 force_glsl_version 直接把 150 → 450 core（不转换关键字）。
    // 验证：attribute/varying 老语法 + location 注入后，在 450 下能否编译
    // （150 的 gl_TexCoord 等已移除内置不在本实验范围）。
    let vs = "#version 450 core\n\
              layout(location=0) attribute vec3 Position;\n\
              layout(location=1) attribute vec4 Color;\n\
              layout(location=0) uniform mat4 ModelViewMat;\n\
              layout(location=1) uniform mat4 ProjMat;\n\
              layout(location=0) varying vec4 vertexColor;\n\
              void main() {\n\
                  gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);\n\
                  vertexColor = Color * vec4(float(gl_VertexID), 0.0, 0.0, 1.0);\n\
              }\n";

    for env in [330u32, 450u32] {
        match compile_opengl(vs, GL_VERTEX_SHADER, env, false) {
            Ok(spv) => {
                println!(
                    "[f] env={} 150 老语法提升 450 + attribute/varying + location: 编译成功 ({} words)",
                    env,
                    spv.len()
                );
                let es = cross_to_gles(&spv, 320).unwrap();
                println!("[f] env={} === GLES 320 产物 ===\n{}", env, es);
                assert!(
                    es.contains("gl_Position"),
                    "[f] env={} 应保留 gl_Position",
                    env
                );
            }
            Err(e) => println!(
                "[f] env={} 记录：attribute/varying 老语法在 450 编译失败 -> {}（需 in/out 关键字迁移）",
                env, e
            ),
        }
    }
}

// ============ 5. 补充：300 es 回退时 uniform location 的处理 ============

#[test]
fn spike_g_gles300_fallback_with_uniform_location() {
    // 管线支持 300 es 回退（gles_version_candidates）。GLSL ES 3.00 不支持
    // standalone uniform 的 layout(location)（ES 3.1+ 特性）。验证 spirv-cross
    // 输出 300 es 时对 uniform location 的行为（报错/省略），
    // 决定 strip_uniform_locations 是否仍需保留。
    let vs = "#version 450 core\n\
              layout(location=0) uniform mat4 ProjMat;\n\
              layout(location=0) in vec3 Position;\n\
              void main() { gl_Position = ProjMat * vec4(Position, 1.0); }\n";
    let spv = compile_opengl(vs, GL_VERTEX_SHADER, 450, false)
        .expect("[g] OpenGL env=450 编译失败");
    // 先确认 320 es 输出正常
    let es320 = cross_to_gles(&spv, 320).unwrap();
    assert!(
        es320.contains("layout(location = 0) uniform mat4 ProjMat"),
        "[g] 320 es 应保留 uniform location，产物:\n{}",
        es320
    );
    // 300 es 输出行为
    match cross_to_gles(&spv, 300) {
        Ok(es300) => {
            println!("[g] 300 es 输出成功：\n{}", es300);
            println!(
                "[g] 300 es 产物 uniform location: {}（若保留 → ES 3.0 编译会失败，strip_uniform_locations 仍需保留）",
                es300.contains("layout(location") && es300.contains("uniform")
            );
        }
        Err(e) => println!(
            "[g] 记录：300 es 输出失败 -> {:?}（300 es 回退需先剥离 uniform location，strip_uniform_locations 仍需保留）",
            e
        ),
    }
}

// ============ 6. 补充：OpenGL target 下 VULKAN 宏的定义行为 ============

#[test]
fn spike_h_vulkan_macro_defined() {
    // preprocess 的 undef_vulkan_macro hack 存在是因为 glslang Vulkan target
    // 自动定义 VULKAN 宏（Sodium 等 mod 用 #ifdef VULKAN 分支）。
    // 验证 OpenGL target 下 VULKAN 宏是否仍被定义（决定 hack 去留）。
    // 探测方法：源码用 `#ifdef VULKAN` 分支给 fragColor 加 vec4(1.0)，
    // 若宏被定义（走 #ifdef 分支），SPIR-V 产物含常量 1.0（0x3F800000）。
    let fs2 = "#version 450 core\n\
               layout(location=0) in vec2 vUV;\n\
               layout(location=0) out vec4 fragColor;\n\
               uniform sampler2D Tex;\n\
               void main() {\n\
               #ifdef VULKAN\n\
                   fragColor = texture(Tex, vUV) + vec4(1.0);\n\
               #else\n\
                   fragColor = texture(Tex, vUV);\n\
               #endif\n\
               }\n";
    let spv = compile_opengl(fs2, GL_FRAGMENT_SHADER, 450, false)
        .unwrap_or_else(|e| panic!("[h] OpenGL env=450 编译失败: {}", e));
    // 区分方法：若走了 #else 分支，产物无 vec4(1.0) 加法；若走 #ifdef 分支则有。
    // 更可靠：走 #else 分支时 constant 1.0 不会出现在产物中（被优化掉？Zero 优化保留）。
    // 直接看 SPIR-V 词法：OpConstant float 1 在 #ifdef 分支产物中存在，否则不存在。
    let spv_words: Vec<String> = spv.iter().map(|w| format!("{:08X}", w)).collect();
    let has_const_1 = spv_words.iter().any(|w| w == "3F800000"); // 1.0f 的 IEEE754
    println!(
        "[h] OpenGL target 下 VULKAN 宏: {}（产物含常量 1.0 = {}；#ifdef 分支命中 = {}）",
        if has_const_1 { "被定义（undef_vulkan_macro 仍需保留）" } else { "未定义（undef_vulkan_macro 可删）" },
        has_const_1,
        has_const_1
    );
    // 对照组：Vulkan target 直接编译（不经 preprocess，避免 undef_vulkan_macro 干扰），
    // 验证 glslang Vulkan 语义确实定义 VULKAN 宏
    let compiler = Compiler::new().expect("[h] Compiler::new failed");
    let mut vk_opts = CompileOptions::new().expect("[h] CompileOptions::new failed");
    vk_opts.set_target_env(shaderc::TargetEnv::Vulkan, shaderc::EnvVersion::Vulkan1_2 as u32);
    vk_opts.set_optimization_level(OptimizationLevel::Zero);
    vk_opts.set_generate_debug_info();
    vk_opts.set_auto_bind_uniforms(true);
    let vk_artifact = compiler
        .compile_into_spirv(fs2, ShaderKind::Fragment, "spike.glsl", "main", Some(&vk_opts))
        .expect("[h] Vulkan target 直接编译失败");
    let spv_vk = vk_artifact.as_binary().to_vec();
    let spv_vk_words: Vec<String> = spv_vk.iter().map(|w| format!("{:08X}", w)).collect();
    let vk_has_const_1 = spv_vk_words.iter().any(|w| w == "3F800000");
    println!(
        "[h] Vulkan target（不经 preprocess）下 VULKAN 宏: {}",
        if vk_has_const_1 { "被定义（验证探测有效）" } else { "未定义" }
    );
    // 关键结论断言：OpenGL target 产物不含 vec4(1.0) 分支常量 → 宏未定义
    assert!(
        !has_const_1,
        "[h] OpenGL target 不应定义 VULKAN 宏（产物不应含 #ifdef 分支常量 1.0）"
    );
    println!("[h] 结论：OpenGL target 下 glslang 不定义 VULKAN 宏 → undef_vulkan_macro 可删");
}
