use crate::backend;

/// glClampColor stub — GL 3.2 固定功能颜色 clamp 控制。
///
/// GLES 总是 clamp 颜色输出（ framebuffer 写入前 clamp 到 [0,1]），
/// 行为与桌面 GL 的 GL_CLAMP_READ_COLOR / GL_FIXED_ONLY 语义一致，
/// 故无需转发，直接 no-op。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClampColor(_target: u32, _clamp: u32) {
    log::debug!(
        "[FluorateGL] glClampColor swallowed (GLES always clamps color output)"
    );
}

/// glPointParameteri — GL 1.4 点光栅化参数（int 版本）。
///
/// 转发到 GLES 的 glPointParameterf（GLES 仅提供 float 版本）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPointParameteri(pname: u32, param: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.point_parameter_f)(pname, param as f32);
    });
}

/// glPointParameteriv — GL 1.4 点光栅化参数（int 数组版本）。
///
/// GLES 仅提供 glPointParameterf，故取数组首元素转为 float 转发。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPointParameteriv(pname: u32, params: *const i32) {
    if params.is_null() {
        return;
    }
    let param = unsafe { *params };
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.point_parameter_f)(pname, param as f32);
    });
}
