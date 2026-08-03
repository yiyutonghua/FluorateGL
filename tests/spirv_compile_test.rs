//! spirv_compile 模块公开 API 单元测试
//!
//! 覆盖 `spirv_compile::compile`、`spirv_compile::map_gl_stage`、
//! `spirv_compile::stage_name` 的各种场景：
//! - GL stage 常量映射（合法/无效）
//! - stage_name 可读名称
//! - 合法 GLSL → SPIR-V 编译成功
//! - 非法 GLSL 返回 None
//! - 无效 stage 返回 None
//! - legacy 版本（< 330）经 preprocess 升级后编译成功
//! - GLSL 330 经预处理升级后编译成功
//! - GL_*_SHADER 常量值正确性
//!
//! 注意：glslang 进程初始化由 `Compiler::acquire()` 内部处理，不依赖 EGL。

use fluorategl::shader_translator::spirv_compile;

// ============ GL shader stage 常量值 ============

#[test]
fn gl_vertex_shader_constant_value() {
    assert_eq!(spirv_compile::GL_VERTEX_SHADER, 0x8B31);
}

#[test]
fn gl_fragment_shader_constant_value() {
    assert_eq!(spirv_compile::GL_FRAGMENT_SHADER, 0x8B30);
}

#[test]
fn gl_geometry_shader_constant_value() {
    assert_eq!(spirv_compile::GL_GEOMETRY_SHADER, 0x8DD9);
}

#[test]
fn gl_tess_control_shader_constant_value() {
    assert_eq!(spirv_compile::GL_TESS_CONTROL_SHADER, 0x8E88);
}

#[test]
fn gl_tess_evaluation_shader_constant_value() {
    assert_eq!(spirv_compile::GL_TESS_EVALUATION_SHADER, 0x8E87);
}

#[test]
fn gl_compute_shader_constant_value() {
    assert_eq!(spirv_compile::GL_COMPUTE_SHADER, 0x91B9);
}

// ============ map_gl_stage ============

#[test]
fn map_gl_stage_vertex() {
    assert!(spirv_compile::map_gl_stage(spirv_compile::GL_VERTEX_SHADER).is_some());
}

#[test]
fn map_gl_stage_fragment() {
    assert!(spirv_compile::map_gl_stage(spirv_compile::GL_FRAGMENT_SHADER).is_some());
}

#[test]
fn map_gl_stage_geometry() {
    assert!(spirv_compile::map_gl_stage(spirv_compile::GL_GEOMETRY_SHADER).is_some());
}

#[test]
fn map_gl_stage_tess_control() {
    assert!(spirv_compile::map_gl_stage(spirv_compile::GL_TESS_CONTROL_SHADER).is_some());
}

#[test]
fn map_gl_stage_tess_evaluation() {
    assert!(spirv_compile::map_gl_stage(spirv_compile::GL_TESS_EVALUATION_SHADER).is_some());
}

#[test]
fn map_gl_stage_compute() {
    assert!(spirv_compile::map_gl_stage(spirv_compile::GL_COMPUTE_SHADER).is_some());
}

#[test]
fn map_gl_stage_invalid_returns_none() {
    assert!(spirv_compile::map_gl_stage(0x0000).is_none());
    assert!(spirv_compile::map_gl_stage(0xFFFF).is_none());
    assert!(spirv_compile::map_gl_stage(0x8B32).is_none()); // 未定义的 stage
}

// ============ stage_name ============

#[test]
fn stage_name_vertex() {
    assert_eq!(
        spirv_compile::stage_name(spirv_compile::GL_VERTEX_SHADER),
        "vertex"
    );
}

#[test]
fn stage_name_fragment() {
    assert_eq!(
        spirv_compile::stage_name(spirv_compile::GL_FRAGMENT_SHADER),
        "fragment"
    );
}

#[test]
fn stage_name_geometry() {
    assert_eq!(
        spirv_compile::stage_name(spirv_compile::GL_GEOMETRY_SHADER),
        "geometry"
    );
}

#[test]
fn stage_name_tess_control() {
    assert_eq!(
        spirv_compile::stage_name(spirv_compile::GL_TESS_CONTROL_SHADER),
        "tess_control"
    );
}

#[test]
fn stage_name_tess_evaluation() {
    assert_eq!(
        spirv_compile::stage_name(spirv_compile::GL_TESS_EVALUATION_SHADER),
        "tess_eval"
    );
}

#[test]
fn stage_name_compute() {
    assert_eq!(
        spirv_compile::stage_name(spirv_compile::GL_COMPUTE_SHADER),
        "compute"
    );
}

#[test]
fn stage_name_invalid_returns_unknown() {
    assert_eq!(spirv_compile::stage_name(0x0000), "unknown");
    assert_eq!(spirv_compile::stage_name(0xFFFF), "unknown");
}

// ============ compile: 合法 GLSL 编译成功 ============

#[test]
fn compile_simple_vertex_shader_succeeds() {
    let src = "#version 330 core\n\
               uniform mat4 MVP;\n\
               in vec3 Position;\n\
               void main() { gl_Position = MVP * vec4(Position, 1.0); }\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER);
    assert!(result.is_some(), "expected SPIR-V output, got None");
    let spv = result.unwrap();
    assert!(!spv.is_empty(), "SPIR-V should not be empty");
    // SPIR-V 魔数检查：0x07230203
    assert_eq!(
        spv[0], 0x07230203,
        "invalid SPIR-V magic number: 0x{:08X}",
        spv[0]
    );
}

#[test]
fn compile_simple_fragment_shader_succeeds() {
    let src = "#version 330 core\n\
               uniform sampler2D tex;\n\
               in vec2 uv;\n\
               out vec4 fragColor;\n\
               void main() { fragColor = texture(tex, uv); }\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_FRAGMENT_SHADER);
    assert!(result.is_some(), "expected SPIR-V output, got None");
    let spv = result.unwrap();
    assert!(!spv.is_empty());
    assert_eq!(spv[0], 0x07230203, "invalid SPIR-V magic number");
}

#[test]
fn compile_vertex_with_ubo_succeeds() {
    let src = "#version 330\n\
               layout(std140) uniform DynamicTransforms {\n\
                   mat4 ModelViewMat;\n\
                   vec4 ColorModulator;\n\
               };\n\
               in vec3 Position;\n\
               in vec4 Color;\n\
               out vec4 vertexColor;\n\
               void main() {\n\
                   gl_Position = ModelViewMat * vec4(Position, 1.0);\n\
                   vertexColor = Color;\n\
               }\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER);
    assert!(result.is_some(), "expected SPIR-V for UBO vertex shader");
}

#[test]
fn compile_fragment_with_multiple_samplers_succeeds() {
    let src = "#version 450\n\
               layout(binding = 0) uniform sampler2D Sampler0;\n\
               layout(binding = 1) uniform sampler2D Sampler1;\n\
               layout(location = 0) in vec2 texCoord;\n\
               layout(location = 0) out vec4 fragColor;\n\
               void main() {\n\
                   vec4 c0 = texture(Sampler0, texCoord);\n\
                   vec4 c1 = texture(Sampler1, texCoord);\n\
                   fragColor = c0 * c1;\n\
               }\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_FRAGMENT_SHADER);
    assert!(
        result.is_some(),
        "expected SPIR-V for multi-sampler fragment shader"
    );
}

#[test]
fn compile_compute_shader_succeeds() {
    let src = "#version 450 core\n\
               layout(local_size_x = 8, local_size_y = 8) in;\n\
               layout(binding = 0, std430) buffer Buffer {\n\
                   vec4 data[];\n\
               };\n\
               void main() {\n\
                   uint idx = gl_GlobalInvocationID.x;\n\
                   data[idx] = vec4(1.0);\n\
               }\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_COMPUTE_SHADER);
    assert!(result.is_some(), "expected SPIR-V for compute shader");
}

#[test]
fn compile_450_shader_succeeds() {
    let src = "#version 450 core\n\
               layout(location = 0) in vec3 pos;\n\
               layout(location = 0) out vec4 color;\n\
               void main() {\n\
                   color = vec4(pos, 1.0);\n\
               }\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER);
    assert!(result.is_some());
}

// ============ compile: 非法 GLSL 返回 None ============

#[test]
fn compile_invalid_glsl_returns_none() {
    let src = "#version 330 core\nthis is not valid glsl\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER);
    assert!(result.is_none(), "expected None for invalid GLSL");
}

#[test]
fn compile_empty_source_returns_none() {
    let result = spirv_compile::compile("", spirv_compile::GL_VERTEX_SHADER);
    // 空源码会被预处理插入 #version 450 core，但无 main 函数，glslang 应拒绝
    assert!(result.is_none(), "expected None for empty source");
}

#[test]
fn compile_syntax_error_returns_none() {
    let src = "#version 330 core\nvoid main() {\n    // 缺少闭括号\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_FRAGMENT_SHADER);
    assert!(result.is_none(), "expected None for syntax error");
}

#[test]
fn compile_undefined_function_returns_none() {
    let src = "#version 330 core\nvoid main() {\n    undefinedFunction();\n}\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_FRAGMENT_SHADER);
    assert!(result.is_none(), "expected None for undefined function");
}

// ============ compile: 版本兼容性 ============

#[test]
fn compile_legacy_120_upgraded_and_succeeds() {
    // #version 120 经 preprocess 升级到 330 core，应编译成功
    let src = "#version 120\nvoid main() {}\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER);
    assert!(result.is_some(), "expected Some for upgraded #version 120");
}

#[test]
fn compile_legacy_150_upgraded_and_succeeds() {
    // #version 150 经 preprocess 升级到 330 core，应编译成功
    let src = "#version 150 core\nvoid main() {}\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER);
    assert!(result.is_some(), "expected Some for upgraded #version 150");
}

#[test]
fn compile_330_is_upgraded_and_succeeds() {
    // #version 330 经预处理升级到 450，应编译成功
    let src = "#version 330\nvoid main() {\n    gl_Position = vec4(1.0);\n}\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER);
    assert!(result.is_some(), "expected 330 to be upgraded and compiled");
}

// ============ compile: 无效 stage ============

#[test]
fn compile_with_invalid_stage_returns_none() {
    let src = "#version 330 core\nvoid main() {}\n";
    let result = spirv_compile::compile(src, 0x0000);
    assert!(result.is_none(), "expected None for invalid stage");
}

#[test]
fn compile_with_undefined_stage_returns_none() {
    let src = "#version 330 core\nvoid main() {}\n";
    let result = spirv_compile::compile(src, 0x8B32); // 未定义的 stage 常量
    assert!(result.is_none(), "expected None for undefined stage");
}

// ============ compile: SPIR-V 结构验证 ============

#[test]
fn compile_produces_valid_spirv_structure() {
    // SPIR-V 二进制格式：
    // - 第 0 个 word：魔数 0x07230203
    // - 第 1 个 word：版本号
    // - 第 2 个 word：生成器 magic number
    // - 第 3 个 word：bound
    // - 第 4 个 word：0（reserved）
    let src = "#version 330 core\nvoid main() {}\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER);
    let spv = result.expect("expected SPIR-V");
    assert!(spv.len() >= 5, "SPIR-V too short: {} words", spv.len());
    assert_eq!(spv[0], 0x07230203, "magic number mismatch");
    assert!(spv[1] != 0, "version should not be 0");
    assert!(spv[3] != 0, "bound should not be 0");
    assert_eq!(spv[4], 0, "reserved word should be 0");
}

#[test]
fn compile_produces_consistent_spirv() {
    // 同一 shader 编译两次应产生相同长度的 SPIR-V
    let src = "#version 330 core\nvoid main() { gl_Position = vec4(1.0); }\n";
    let spv1 = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER).unwrap();
    let spv2 = spirv_compile::compile(src, spirv_compile::GL_VERTEX_SHADER).unwrap();
    assert_eq!(spv1.len(), spv2.len(), "SPIR-V length should be consistent");
    assert_eq!(spv1, spv2, "SPIR-V content should be consistent");
}

// ============ samplerBuffer 编译成功（clouds 回归） ============

#[test]
fn test_compile_sampler_buffer_success() {
    // clouds 特征：fragment stage + isamplerBuffer + int 坐标 texelFetch。
    // 修复前（convert_sampler_buffer 把 samplerBuffer 改写为 isampler2D +
    // u_BufferTexWidth）shaderc Vulkan target 下编译失败返回 None；
    // T1 禁用后 samplerBuffer 原样保留，应编译成功。
    let src = "#version 330\n\
               uniform isamplerBuffer CloudFaces;\n\
               in vec2 vUV;\n\
               out vec4 fragColor;\n\
               void main() {\n\
                   int index = int(gl_FragCoord.x);\n\
                   vec4 color = vec4(texelFetch(CloudFaces, index));\n\
                   fragColor = color;\n\
               }\n";
    let result = spirv_compile::compile(src, spirv_compile::GL_FRAGMENT_SHADER);
    assert!(result.is_some(), "expected SPIR-V output, got None");
    let spv = result.unwrap();
    assert!(!spv.is_empty(), "SPIR-V should not be empty");
    // SPIR-V 魔数检查：0x07230203
    assert_eq!(
        spv[0], 0x07230203,
        "invalid SPIR-V magic number: 0x{:08X}",
        spv[0]
    );
}
