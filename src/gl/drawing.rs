use crate::backend;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawRangeElements(
    mode: u32,
    start: u32,
    end: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_range_elements)(mode, start, end, count, type_, indices);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysInstanced(mode: u32, first: i32, count: i32, instancecount: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_arrays_instanced)(mode, first, count, instancecount);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsInstanced(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    instancecount: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_elements_instanced)(mode, count, type_, indices, instancecount);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPrimitiveRestartIndex(index: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if !is_stub(dispatch, dispatch.primitive_restart_index as *const ()) {
            (dispatch.primitive_restart_index)(index);
        }
        // GLES 不支持此函数时静默忽略
    });
}

/// Returns `true` if the dispatch function pointer is the shared unimplemented stub.
///
/// `load_opt!` substitutes missing optional functions with a single stub function,
/// so every stub field in `GlesDispatch` has the same address. We compare against
/// `dispatch.stub`, which is initialized to that exact stub address.
fn is_stub(dispatch: &backend::dispatch::GlesDispatch, ptr: *const ()) -> bool {
    ptr == dispatch.stub as *const ()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawArrays(
    mode: u32,
    first: *const i32,
    count: *const i32,
    drawcount: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.multi_draw_arrays as *const ()) {
            for i in 0..drawcount as isize {
                (dispatch.draw_arrays)(mode, *first.offset(i), *count.offset(i));
            }
        } else {
            (dispatch.multi_draw_arrays)(mode, first, count, drawcount);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawElements(
    mode: u32,
    count: *const i32,
    type_: u32,
    indices: *const *const std::ffi::c_void,
    drawcount: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.multi_draw_elements as *const ()) {
            for i in 0..drawcount as isize {
                (dispatch.draw_elements)(mode, *count.offset(i), type_, *indices.offset(i));
            }
        } else {
            (dispatch.multi_draw_elements)(mode, count, type_, indices, drawcount);
        }
    });
}
