use crate::backend;
use crate::state;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenVertexArrays(n: i32, arrays: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let mut gles_id = 0u32;
            (dispatch.gen_vertex_arrays)(1, &mut gles_id);
            
            let desktop_id = state::with_state(|s| s.vertex_arrays.alloc(gles_id));
            *arrays.offset(i) = desktop_id;
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteVertexArrays(n: i32, arrays: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let desktop_id = *arrays.offset(i);
            if let Some(gles_id) = state::with_state(|s| s.vertex_arrays.delete(desktop_id)) {
                (dispatch.delete_vertex_arrays)(1, &gles_id);
            }
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindVertexArray(array: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if array == 0 {
            0
        } else {
            state::with_state(|s| s.vertex_arrays.get_gles(array).unwrap_or(0))
        };
        
        (dispatch.bind_vertex_array)(gles_id);
        state::with_state(|s| s.bound_vertex_array = array);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEnableVertexAttribArray(index: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.enable_vertex_attrib_array)(index);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDisableVertexAttribArray(index: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.disable_vertex_attrib_array)(index);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribPointer(
    index: u32,
    size: i32,
    type_: u32,
    normalized: u8,
    stride: i32,
    pointer: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_pointer)(index, size, type_, normalized, stride, pointer);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribIPointer(
    index: u32,
    size: i32,
    type_: u32,
    stride: i32,
    pointer: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_pointer)(index, size, type_, stride, pointer);
    });
}
