//! 独立测试：验证 GLSL -> SPIR-V -> GLSL ES 翻译管线
//! 运行：cargo run --example translate_test

use fluorategl::shader_translator::spirv_pass::{TranslationResult, translate};

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;

fn test_translate(name: &str, source: &str, stage: u32) {
    println!("\n========== 测试 {} ==========", name);
    println!("[输入] GLSL 源码:\n{}", source);

    let result = translate(source, stage);
    match result {
        TranslationResult::Translated(src) => {
            println!("[成功] 翻译后 GLSL ES ({} chars):\n{}", src.len(), src);
        }
        TranslationResult::PassThrough => {
            println!("[透传] 驱动扩展支持，未翻译");
        }
        TranslationResult::Failed => {
            println!("[失败] 翻译失败");
        }
    }
}

fn main() {
    // 初始化 FluorateGL（glslang 需要初始化进程）
    let ret = fluorategl::fluorategl_init();
    println!("fluorategl_init 返回: {}", ret);

    // 测试 1：简单 fragment shader
    test_translate(
        "simple.frag",
        r#"#version 330 core
in vec2 vUV;
out vec4 fragColor;
uniform sampler2D Tex;
void main() {
    fragColor = texture(Tex, vUV);
}
"#,
        GL_FRAGMENT_SHADER,
    );

    // 测试 2：带 UBO 的 vertex shader（Minecraft 风格）
    test_translate(
        "mc_style.vert",
        r#"#version 330
layout(std140) uniform DynamicTransforms {
    mat4 ModelViewMat;
    vec4 ColorModulator;
    vec3 ModelOffset;
    mat4 TextureMat;
};
in vec3 Position;
in vec4 Color;
out vec4 vertexColor;
void main() {
    gl_Position = ModelViewMat * vec4(Position, 1.0);
    vertexColor = Color;
}
"#,
        GL_VERTEX_SHADER,
    );

    // 测试 3：带 layout(binding) 的 fragment shader
    test_translate(
        "binding.frag",
        r#"#version 450
layout(binding = 0) uniform sampler2D Sampler0;
layout(binding = 1) uniform sampler2D Sampler1;
layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;
void main() {
    vec4 c0 = texture(Sampler0, texCoord);
    vec4 c1 = texture(Sampler1, texCoord);
    fragColor = c0 * c1;
}
"#,
        GL_FRAGMENT_SHADER,
    );

    // 测试 4：实际 Minecraft 日志中的 shader 源码（失败的那个）
    test_translate(
        "mc_real.frag",
        r#"#version 330

layout(std140) uniform DynamicTransforms {
    mat4 ModelViewMat;
    vec4 ColorModulator;
    vec3 ModelOffset;
    mat4 TextureMat;
};

in vec4 vertexColor;

out vec4 fragColor;

void main() {
    vec4 color = vertexColor;
    if (color.a == 0.0) {
        discard;
    }
    fragColor = color * ColorModulator;
}
"#,
        GL_FRAGMENT_SHADER,
    );
}
