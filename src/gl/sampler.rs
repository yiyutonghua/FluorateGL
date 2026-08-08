//! Sampler 对象家族导出（GL 3.3 core 补齐）
//!
//! GL 3.3 引入 sampler 对象（glGenSamplers/glBindSampler/glSamplerParameter*），
//! GLES 3.0 原生支持同名函数——全部透传。此前 dispatch 已有 9 个字段但未导出
//! 符号，LWJGL 在 3.3 下加载 sampler 函数（config.rs 注释依据）绑定 null 有
//! 崩溃风险（Sodium 创建 sampler 对象时触发）。

use crate::backend;

macro_rules! passthrough_fn {
    ($name:ident, $field:ident, $($arg:ident: $ty:ty),*) => {
        #[unsafe(no_mangle)]
        #[allow(non_snake_case)]
        pub extern "C" fn $name($($arg: $ty),*) {
            backend::with_gles_dispatch(|dispatch| unsafe {
                (dispatch.$field)($($arg),*);
            });
        }
    };
}

passthrough_fn!(glGenSamplers, gen_samplers, count: i32, samplers: *mut u32);
passthrough_fn!(glDeleteSamplers, delete_samplers, count: i32, samplers: *const u32);
passthrough_fn!(glBindSampler, bind_sampler, unit: u32, sampler: u32);
passthrough_fn!(glSamplerParameteri, sampler_parameter_i, sampler: u32, pname: u32, param: i32);
passthrough_fn!(glSamplerParameterf, sampler_parameter_f, sampler: u32, pname: u32, param: f32);
passthrough_fn!(glSamplerParameteriv, sampler_parameter_iv, sampler: u32, pname: u32, param: *const i32);
passthrough_fn!(glSamplerParameterfv, sampler_parameter_fv, sampler: u32, pname: u32, param: *const f32);
passthrough_fn!(glSamplerParameterIiv, sampler_parameter_i_iv, sampler: u32, pname: u32, param: *const i32);
passthrough_fn!(glSamplerParameterIuiv, sampler_parameter_i_uiv, sampler: u32, pname: u32, param: *const u32);
passthrough_fn!(glGetSamplerParameteriv, get_sampler_parameter_iv, sampler: u32, pname: u32, params: *mut i32);
passthrough_fn!(glGetSamplerParameterfv, get_sampler_parameter_fv, sampler: u32, pname: u32, params: *mut f32);
passthrough_fn!(glGetSamplerParameterIiv, get_sampler_parameter_i_iv, sampler: u32, pname: u32, params: *mut i32);
passthrough_fn!(glGetSamplerParameterIuiv, get_sampler_parameter_i_uiv, sampler: u32, pname: u32, params: *mut u32);

// glIsSampler 返回 u8，宏不适用，手写：
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsSampler(sampler: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.is_sampler)(sampler) })
}
