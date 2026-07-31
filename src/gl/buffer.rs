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
            log::debug!(
                "[FluorateGL] glGenBuffers: GLES {} -> desktop {} (tid={})",
                gles_id,
                desktop_id,
                state::thread_id_u64()
            );
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
                log::debug!(
                    "[FluorateGL] glDeleteBuffers: desktop {} -> GLES {} (deleted, tid={})",
                    desktop_id,
                    gles_id,
                    state::thread_id_u64()
                );
                (dispatch.delete_buffers)(1, &gles_id);
            } else {
                log::debug!(
                    "[FluorateGL] glDeleteBuffers: desktop {} NOT FOUND in IdMap, ignored",
                    desktop_id
                );
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
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                log::warn!(
                    "[FluorateGL] glBindBuffer(0x{:04X}, {}): desktop ID not found in IdMap, unbinding",
                    target, buffer
                );
                0
            })
            })
        };

        if buffer != 0 && gles_id != 0 {
            log::debug!(
                "[FluorateGL] glBindBuffer(0x{:04X}): desktop {} -> GLES {} (tid={})",
                target,
                buffer,
                gles_id,
                state::thread_id_u64()
            );
        }

        (dispatch.bind_buffer)(target, gles_id);

        if target == 0x8892 || target == 0x8893 {
            // GL_ARRAY_BUFFER or GL_ELEMENT_ARRAY_BUFFER
            state::with_state(|s| s.bound_buffer = buffer);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferData(
    target: u32,
    size: isize,
    data: *const std::ffi::c_void,
    usage: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.buffer_data)(target, size, data, usage);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferSubData(
    target: u32,
    offset: isize,
    size: isize,
    data: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.buffer_sub_data)(target, offset, size, data);
    });
}

fn is_stub(dispatch: &backend::dispatch::GlesDispatch, f: *const ()) -> bool {
    f == dispatch.stub as *const ()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferStorage(
    target: u32,
    size: isize,
    data: *const std::ffi::c_void,
    flags: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 注意：这里 is_stub 的签名请确保和 drawing.rs 里一致（推荐用 *const ()）
        if is_stub(dispatch, dispatch.buffer_storage as *const ()) {
            // 如果底层真的没有 glBufferStorage，只能降级为 glBufferData
            (dispatch.buffer_data)(target, size, data, 0x88E8); // GL_DYNAMIC_DRAW
        } else {
            // ✅ 修复：原样传入 flags！不要剥离 PERSISTENT 和 COHERENT！
            // MC 的顶点流式上传完全依赖这两个 Bit。
            (dispatch.buffer_storage)(target, size, data, flags);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMapBuffer(target: u32, access: u32) -> *mut std::ffi::c_void {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // glMapBuffer is not available in GLES 3.0; emulate it with glMapBufferRange.
        // 若 map_buffer_range 也是 stub（驱动不支持），返回 null 避免后续 UB。
        if is_stub(dispatch, dispatch.map_buffer_range as *const ()) {
            log::warn!("[FluorateGL] glMapBuffer: glMapBufferRange not available, returning null");
            return std::ptr::null_mut();
        }

        let mut size = 0i32;
        (dispatch.get_buffer_parameter_iv)(target, 0x8764, &mut size); // GL_BUFFER_SIZE

        // size 为负或零时无意义，直接返回 null
        if size <= 0 {
            log::warn!(
                "[FluorateGL] glMapBuffer: invalid buffer size {}, returning null",
                size
            );
            return std::ptr::null_mut();
        }

        let range_access = match access {
            0x88B8 => 0x0001,          // GL_READ_ONLY -> GL_MAP_READ_BIT
            0x88B9 => 0x0002,          // GL_WRITE_ONLY -> GL_MAP_WRITE_BIT
            0x88BA => 0x0001 | 0x0002, // GL_READ_WRITE -> GL_MAP_READ_BIT | GL_MAP_WRITE_BIT
            _ => access,
        };

        (dispatch.map_buffer_range)(target, 0, size as isize, range_access)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMapBufferRange(
    target: u32,
    offset: isize,
    length: isize,
    access: u32,
) -> *mut std::ffi::c_void {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // ✅ 修复：原样传入 access！不要剥离 PERSISTENT 和 COHERENT！
        (dispatch.map_buffer_range)(target, offset, length, access)
    })
}
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUnmapBuffer(target: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.unmap_buffer)(target) })
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
pub extern "C" fn glCopyBufferSubData(
    readTarget: u32,
    writeTarget: u32,
    readOffset: isize,
    writeOffset: isize,
    size: isize,
) {
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
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                log::warn!(
                    "[FluorateGL] glBindBufferBase(0x{:04X}, {}): desktop ID {} not found in IdMap, unbinding",
                    target, index, buffer
                );
                0
            })
            })
        };

        (dispatch.bind_buffer_base)(target, index, gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindBufferRange(
    target: u32,
    index: u32,
    buffer: u32,
    offset: isize,
    size: isize,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                log::warn!(
                    "[FluorateGL] glBindBufferRange(0x{:04X}, {}): desktop ID {} not found in IdMap, unbinding",
                    target, index, buffer
                );
                0
            })
            })
        };

        (dispatch.bind_buffer_range)(target, index, gles_id, offset, size);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferSubData(
    target: u32,
    offset: isize,
    size: isize,
    data: *mut std::ffi::c_void,
) {
    // size/offset 为负或 data 为空时无意义，直接返回
    if data.is_null() || size <= 0 || offset < 0 {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.get_buffer_sub_data as *const ()) {
            // GLES 没有 glGetBufferSubData，用 MapBufferRange 模拟
            if is_stub(dispatch, dispatch.map_buffer_range as *const ()) {
                log::warn!(
                    "[FluorateGL] glGetBufferSubData: both sub_data and map_range unavailable"
                );
                return;
            }
            let ptr = (dispatch.map_buffer_range)(
                target, offset, size, 0x0001, /* GL_MAP_READ_BIT */
            );
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(ptr, data, size as usize);
                (dispatch.unmap_buffer)(target);
            }
        } else {
            (dispatch.get_buffer_sub_data)(target, offset, size, data);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferParameteriv(target: u32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_buffer_parameter_iv)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferPointerv(target: u32, pname: u32, params: *mut *mut std::ffi::c_void) {
    if params.is_null() {
        return;
    }
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

// === GL_EXT_texture_buffer / GLES 3.2 ===
// glTexBuffer 将 buffer 绑定到纹理，buffer ID 需要从 desktop 翻译为 GLES。

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexBuffer(target: u32, internalformat: u32, buffer: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.tex_buffer as *const ()) {
            log::warn!("[FluorateGL] glTexBuffer: stub, ignored");
            return;
        }

        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                    log::warn!(
                        "[FluorateGL] glTexBuffer(target=0x{:04X}): desktop ID {} not found in IdMap, unbinding",
                        target, buffer
                    );
                    0
                })
            })
        };

        log::debug!(
            "[FluorateGL] glTexBuffer(target=0x{:04X}, fmt=0x{:04X}) desktop {} -> GLES {} (tid={})",
            target,
            internalformat,
            buffer,
            gles_id,
            state::thread_id_u64()
        );

        (dispatch.tex_buffer)(target, internalformat, gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexBufferRange(
    target: u32,
    internalformat: u32,
    buffer: u32,
    offset: isize,
    size: isize,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.tex_buffer_range as *const ()) {
            log::warn!("[FluorateGL] glTexBufferRange: stub, ignored");
            return;
        }

        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                    log::warn!(
                        "[FluorateGL] glTexBufferRange(target=0x{:04X}): desktop ID {} not found in IdMap, unbinding",
                        target, buffer
                    );
                    0
                })
            })
        };

        log::debug!(
            "[FluorateGL] glTexBufferRange(target=0x{:04X}, fmt=0x{:04X}) desktop {} -> GLES {} (tid={})",
            target,
            internalformat,
            buffer,
            gles_id,
            state::thread_id_u64()
        );

        (dispatch.tex_buffer_range)(target, internalformat, gles_id, offset, size);
    });
}
