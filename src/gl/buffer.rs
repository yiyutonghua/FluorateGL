use crate::backend;
use crate::state;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenBuffers(n: i32, buffers: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let mut gles_id = 0u32;
            (dispatch.gen_buffers)(1, &mut gles_id);
            
            let desktop_id = state::with_state(|s| s.buffers.alloc(gles_id));
            *buffers.offset(i) = desktop_id;
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteBuffers(n: i32, buffers: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let desktop_id = *buffers.offset(i);
            if let Some(gles_id) = state::with_state(|s| s.buffers.delete(desktop_id)) {
                (dispatch.delete_buffers)(1, &gles_id);
            }
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindBuffer(target: u32, buffer: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| s.buffers.get_gles(buffer).unwrap_or(0))
        };
        
        (dispatch.bind_buffer)(target, gles_id);
        
        if target == 0x8892 || target == 0x8893 { // GL_ARRAY_BUFFER or GL_ELEMENT_ARRAY_BUFFER
            state::with_state(|s| s.bound_buffer = buffer);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferData(target: u32, size: isize, data: *const std::ffi::c_void, usage: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.buffer_data)(target, size, data, usage);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferSubData(target: u32, offset: isize, size: isize, data: *const std::ffi::c_void) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.buffer_sub_data)(target, offset, size, data);
    });
}

