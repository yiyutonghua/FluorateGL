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

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMapBuffer(target: u32, access: u32) -> *mut std::ffi::c_void {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // glMapBuffer is not available in GLES 3.0; emulate it with glMapBufferRange.
        let mut size = 0i32;
        (dispatch.get_buffer_parameter_iv)(target, 0x8764, &mut size); // GL_BUFFER_SIZE

        let range_access = match access {
            0x88B8 => 0x0001,                   // GL_READ_ONLY -> GL_MAP_READ_BIT
            0x88B9 => 0x0002,                   // GL_WRITE_ONLY -> GL_MAP_WRITE_BIT
            0x88BA => 0x0001 | 0x0002,          // GL_READ_WRITE -> GL_MAP_READ_BIT | GL_MAP_WRITE_BIT
            _ => access,
        };

        (dispatch.map_buffer_range)(target, 0, size as isize, range_access)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMapBufferRange(target: u32, offset: isize, length: isize, access: u32) -> *mut std::ffi::c_void {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.map_buffer_range)(target, offset, length, access)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUnmapBuffer(target: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.unmap_buffer)(target)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFlushMappedBufferRange(target: u32, offset: isize, length: isize) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.flush_mapped_buffer_range)(target, offset, length);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCopyBufferSubData(readTarget: u32, writeTarget: u32, readOffset: isize, writeOffset: isize, size: isize) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.copy_buffer_sub_data)(readTarget, writeTarget, readOffset, writeOffset, size);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindBufferBase(target: u32, index: u32, buffer: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| s.buffers.get_gles(buffer).unwrap_or(0))
        };

        (dispatch.bind_buffer_base)(target, index, gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindBufferRange(target: u32, index: u32, buffer: u32, offset: isize, size: isize) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| s.buffers.get_gles(buffer).unwrap_or(0))
        };

        (dispatch.bind_buffer_range)(target, index, gles_id, offset, size);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferSubData(target: u32, offset: isize, size: isize, data: *mut std::ffi::c_void) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_buffer_sub_data)(target, offset, size, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferParameteriv(target: u32, pname: u32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_buffer_parameter_iv)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferPointerv(target: u32, pname: u32, params: *mut *mut std::ffi::c_void) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_buffer_pointer_v)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsBuffer(buffer: u32) -> u8 {
    if buffer == 0 {
        return 0;
    }

    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.buffers.get_gles(buffer).unwrap_or(0));
        (dispatch.is_buffer)(gles_id)
    })
}

