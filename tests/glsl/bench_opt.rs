//! SPIRV-Tools 优化管线性能基准（S3-2）
//!
//! 对比 shaderc-only 与 shaderc+opt 的耗时 / SPIR-V word 数 / GLES 输出行数。
//! 用法：cargo run --example bench_opt（或 tools/bench_opt.sh 包装）
//!
//! 样本：5 个代表性 shader（standalone uniform / UBO / sampler / 死代码 / 复杂）。

use std::time::Instant;

use fluorategl::shader_translator::{gles_compile, spirv_compile, spirv_opt};

const SAMPLES: &[(&str, &str, u32)] = &[
    (
        "simple_vs",
        r#"#version 450 core
layout(location=0) in vec3 Position;
layout(location=1) in vec2 UV;
layout(location=0) out vec2 vUV;
layout(location=0) uniform mat4 ModelViewProjection;
void main() { vUV = UV; gl_Position = ModelViewProjection * vec4(Position, 1.0); }
"#,
        spirv_compile::GL_VERTEX_SHADER,
    ),
    (
        "simple_fs",
        r#"#version 450 core
layout(location=0) in vec2 vUV;
layout(location=0) out vec4 fragColor;
layout(binding=0) uniform sampler2D Tex;
void main() { fragColor = texture(Tex, vUV); }
"#,
        spirv_compile::GL_FRAGMENT_SHADER,
    ),
    (
        "ubo_vs",
        r#"#version 450 core
layout(std140, binding=0) uniform Projection { mat4 ProjMat; };
layout(std140, binding=1) uniform DynamicTransforms { mat4 ModelViewMat; vec4 ColorModulator; };
layout(location=0) in vec3 Position;
layout(location=0) out vec4 vertexColor;
void main() {
    gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);
    vertexColor = ColorModulator;
}
"#,
        spirv_compile::GL_VERTEX_SHADER,
    ),
    (
        "dead_fs",
        r#"#version 450 core
layout(location=0) in vec2 vUV;
layout(location=0) out vec4 fragColor;
layout(binding=0) uniform sampler2D Tex;
layout(location=1) uniform float UnusedFloat;
layout(location=2) uniform vec4 Tint;
void main() {
    vec4 c = texture(Tex, vUV);
    float dead = UnusedFloat * 2.0;
    c *= Tint;
    fragColor = c;
}
"#,
        spirv_compile::GL_FRAGMENT_SHADER,
    ),
    (
        "complex_vs",
        r#"#version 450 core
layout(location=0) in vec3 Position;
layout(location=1) in vec2 UV;
layout(location=0) out vec2 vUV;
layout(location=1) out float vDepth;
layout(location=0) uniform mat4 ModelViewProjection;
layout(location=1) uniform float Time;
float helper(float x) { if (x > 0.5) { return x * 2.0; } return x; }
void main() {
    vUV = UV;
    float t = helper(Time);
    vDepth = t;
    gl_Position = ModelViewProjection * vec4(Position, 1.0);
}
"#,
        spirv_compile::GL_VERTEX_SHADER,
    ),
];

fn main() {
    println!("== 优化管线性能基准（shaderc-only vs shaderc+opt，3 次平均）==");
    println!(
        "{:<12} {:>10} {:>10} {:>10} {:>12} {:>12}",
        "sample", "shaderc", "+opt", "opt占比", "words→", "GLES行→"
    );

    for (name, src, stage) in SAMPLES {
        // shaderc 编译（含 preprocess）——3 次取平均
        let mut sc_total = std::time::Duration::ZERO;
        let mut spv = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            spv = spirv_compile::compile(src, *stage).expect("shaderc fail");
            sc_total += t.elapsed();
        }
        let sc_avg = sc_total / 3;

        // shaderc + opt 总耗时（3 次）
        let mut full_total = std::time::Duration::ZERO;
        let mut opt_spv = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            let s = spirv_compile::compile(src, *stage).expect("shaderc fail");
            opt_spv = spirv_opt::run(&s).expect("opt fail");
            full_total += t.elapsed();
        }
        let full_avg = full_total / 3;
        let opt_overhead = full_avg.saturating_sub(sc_avg);

        // GLES 输出行数（有无 opt）
        let gles_plain = gles_compile::compile(&spv, 320).expect("cross fail");
        let gles_opt = gles_compile::compile(&opt_spv, 320).expect("cross fail");

        println!(
            "{:<12} {:>8.1?} {:>8.1?} {:>7.1}% {:>6}→{:<6} {:>4}→{:<4}",
            name,
            sc_avg,
            full_avg,
            opt_overhead.as_secs_f64() / full_avg.as_secs_f64() * 100.0,
            spv.len(),
            opt_spv.len(),
            gles_plain.lines().count(),
            gles_opt.lines().count(),
        );
    }
}
