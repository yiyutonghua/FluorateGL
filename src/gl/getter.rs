use crate::backend;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBooleanv(pname: u32, data: *mut u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_boolean_v)(pname, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFloatv(pname: u32, data: *mut f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_float_v)(pname, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetDoublev(pname: u32, data: *mut f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_double_v)(pname, data);
    });
}

/// `glGetIntegerv` 的透传版本，供 exports.rs 中特殊处理回退时调用，
/// 避免在 getter.rs 与 exports.rs 中重复导出 C 符号。
pub fn get_integerv(pname: u32, data: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_integerv)(pname, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetInteger64v(pname: u32, data: *mut i64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_integer_64v)(pname, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBooleani_v(target: u32, index: u32, data: *mut u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_booleani_v)(target, index, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetIntegeri_v(target: u32, index: u32, data: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_integeri_v)(target, index, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFloati_v(target: u32, index: u32, data: *mut f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_floati_v)(target, index, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetDoublei_v(target: u32, index: u32, data: *mut f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_doublei_v)(target, index, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsEnabled(cap: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.is_enabled)(cap) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsEnabledi(cap: u32, index: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.is_enabled_i)(cap, index) })
}
