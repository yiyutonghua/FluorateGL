//! SPIRV-Tools Optimizer 接入模块（阶段 1）
//!
//! shaderc(OpenGL 450/Zero) → 本模块（AggressiveDCE + RemoveUnusedInterfaceVariables）
//! → spirv-cross。Spike 已验证（TODO/spirv_opt_pipeline.md + /tmp/spirv-spike）：
//! 活跃 uniform/UBO 成员名全保留、diff 仅死代码移除、opt 平均 638µs。
//!
//! 关键决策（spike 修正项）：
//! - **env 用 `TargetEnv::Universal_1_5`**——我们的 shaderc 输出 SPIR-V 1.5，
//!   Vulkan_1_1（上限 1.3）与 OpenGL_4_5（上限 1.0）均拒绝该输入（实测报错）
//! - pass 链固定 AggressiveDCE + RemoveUnusedInterfaceVariables（阶段 2 才加更多）
//! - `OPT_PIPELINE_VERSION` 随 pass 链变更递增（cache key 用，防旧缓存错误命中）

use spirv_tools::opt::Optimizer;
use std::fmt;

/// SPIR-V magic number
const SPIRV_MAGIC: u32 = 0x0723_0203;

/// pass 链版本：变更 pass 链时必须递增（cache.rs compute_key 依赖）
/// v1: AggressiveDCE + RemoveUnusedInterfaceVariables
/// v2: + EliminateDeadConstant（S2-2）
pub const OPT_PIPELINE_VERSION: u32 = 2;

/// 优化失败错误（fail-open：调用方收到 Err 后回退原始 SPIR-V）
#[derive(Debug)]
pub enum OptError {
    /// 空输入（无内容可优化）
    EmptyInput,
    /// 输入不是合法 SPIR-V（magic 不匹配）
    BadMagic,
    /// spirv-tools optimize 内部失败（含原始错误信息）
    Optimize(String),
}

impl fmt::Display for OptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptError::EmptyInput => write!(f, "empty SPIR-V input"),
            OptError::BadMagic => write!(f, "invalid SPIR-V magic number"),
            OptError::Optimize(e) => write!(f, "optimize failed: {}", e),
        }
    }
}

/// 对 SPIR-V 模块运行固定 pass 链（AggressiveDCE + RemoveUnusedInterfaceVariables
/// + EliminateDeadConstant）。
///
/// 返回优化后的 SPIR-V words；任何失败返回 `Err`——调用方（spirv_pass.rs）应
/// fail-open 回退原始 SPIR-V（永不劣化不变式）。
pub fn run(spv: &[u32]) -> Result<Vec<u32>, OptError> {
    if spv.is_empty() {
        return Err(OptError::EmptyInput);
    }
    if spv[0] != SPIRV_MAGIC {
        return Err(OptError::BadMagic);
    }

    let start = std::time::Instant::now();

    // word 数组 → Binary（spike 验证：直接 AsRef<[u32]> 输入即可）
    let mut optimizer = spirv_tools::opt::create(Some(spirv_tools::TargetEnv::Universal_1_5));
    optimizer.register_pass(spirv_tools::opt::Passes::AggressiveDCE);
    optimizer.register_pass(spirv_tools::opt::Passes::RemoveUnusedInterfaceVariables);
    // S2-2：删未使用的常量（低风险——活跃常量/名字不变）
    optimizer.register_pass(spirv_tools::opt::Passes::EliminateDeadConstant);

    let mut msg_cb = |msg: spirv_tools::error::Message| {
        log::debug!(
            "[ShaderTranslator] spirv-opt msg: level={:?} text={}",
            msg.level,
            msg.message
        );
    };

    let bin = optimizer
        .optimize(spv, &mut msg_cb, None)
        .map_err(|e| OptError::Optimize(e.to_string()))?;

    log::debug!(
        "[ShaderTranslator] spirv-opt: {} words -> {} words, took {:?}",
        spv.len(),
        bin.as_words().len(),
        start.elapsed()
    );

    Ok(bin.as_words().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空输入 → EmptyInput
    #[test]
    fn empty_input_returns_err() {
        assert!(matches!(run(&[]), Err(OptError::EmptyInput)));
    }

    /// 坏 magic → BadMagic
    #[test]
    fn bad_magic_returns_err() {
        assert!(matches!(run(&[0xDEAD_BEEF, 0, 0]), Err(OptError::BadMagic)));
    }

    /// 合法 shader（经 shaderc 编译）→ Ok 且输出 magic 合法
    #[test]
    fn valid_shader_optimizes() {
        let src = r#"#version 450 core
layout(location=0) in vec3 Position;
layout(location=1) in vec2 UV;
layout(location=0) out vec2 vUV;
layout(location=0) uniform mat4 ModelViewProjection;
void main() {
    vUV = UV;
    gl_Position = ModelViewProjection * vec4(Position, 1.0);
}
"#;
        let compiler = shaderc::Compiler::new().unwrap();
        let mut opts = shaderc::CompileOptions::new().unwrap();
        opts.set_target_env(shaderc::TargetEnv::OpenGL, 450);
        opts.set_optimization_level(shaderc::OptimizationLevel::Zero);
        opts.set_generate_debug_info();
        opts.set_auto_bind_uniforms(true);
        opts.set_target_spirv(shaderc::SpirvVersion::V1_5);
        let art = compiler
            .compile_into_spirv(
                src,
                shaderc::ShaderKind::Vertex,
                "shader.glsl",
                "main",
                Some(&opts),
            )
            .expect("shaderc compile failed");
        let spv: Vec<u32> = art.as_binary().to_vec();
        assert_eq!(spv[0], SPIRV_MAGIC);

        let out = run(&spv).expect("optimize should succeed");
        assert_eq!(out[0], SPIRV_MAGIC, "优化输出 magic 应合法");
        assert!(out.len() <= spv.len(), "DCE 不应增大模块");
    }
}
