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
            state::with_state(|s| {
                s.vertex_arrays.get_gles(array).unwrap_or_else(|| {
                log::warn!(
                    "[FluorateGL] glBindVertexArray({}): desktop ID not found in IdMap, unbinding",
                    array
                );
                0
            })
            })
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
    let bound_buffer = state::with_state(|s| s.bound_buffer);
    let bound_vao = state::with_state(|s| s.bound_vertex_array);
    log::debug!(
        "[FluorateGL] glVertexAttribPointer(index={}, size={}, type=0x{:04X}, norm={}, stride={}, ptr={:?}) bound_buf={} bound_vao={} (tid={})",
        index,
        size,
        type_,
        normalized,
        stride,
        pointer,
        bound_buffer,
        bound_vao,
        state::thread_id_u64()
    );
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

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribDivisor(index: u32, divisor: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_divisor)(index, divisor);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib1f(index: u32, x: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_1f)(index, x);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib2f(index: u32, x: f32, y: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_2f)(index, x, y);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib3f(index: u32, x: f32, y: f32, z: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_3f)(index, x, y, z);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4f(index: u32, x: f32, y: f32, z: f32, w: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(index, x, y, z, w);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib1fv(index: u32, v: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_1fv)(index, v);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib2fv(index: u32, v: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_2fv)(index, v);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib3fv(index: u32, v: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_3fv)(index, v);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4fv(index: u32, v: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4fv)(index, v);
    });
}

fn is_stub(dispatch: &backend::dispatch::GlesDispatch, f: *const ()) -> bool {
    f == dispatch.stub as *const ()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI1i(index: u32, x: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_1i as *const ()) {
            (dispatch.vertex_attrib_i_4i)(index, x, 0, 0, 0);
        } else {
            (dispatch.vertex_attrib_i_1i)(index, x);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI2i(index: u32, x: i32, y: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_2i as *const ()) {
            (dispatch.vertex_attrib_i_4i)(index, x, y, 0, 0);
        } else {
            (dispatch.vertex_attrib_i_2i)(index, x, y);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI3i(index: u32, x: i32, y: i32, z: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_3i as *const ()) {
            (dispatch.vertex_attrib_i_4i)(index, x, y, z, 0);
        } else {
            (dispatch.vertex_attrib_i_3i)(index, x, y, z);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI4i(index: u32, x: i32, y: i32, z: i32, w: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_4i)(index, x, y, z, w);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI1ui(index: u32, x: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_1ui as *const ()) {
            (dispatch.vertex_attrib_i_4ui)(index, x, 0, 0, 0);
        } else {
            (dispatch.vertex_attrib_i_1ui)(index, x);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI2ui(index: u32, x: u32, y: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_2ui as *const ()) {
            (dispatch.vertex_attrib_i_4ui)(index, x, y, 0, 0);
        } else {
            (dispatch.vertex_attrib_i_2ui)(index, x, y);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI3ui(index: u32, x: u32, y: u32, z: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_3ui as *const ()) {
            (dispatch.vertex_attrib_i_4ui)(index, x, y, z, 0);
        } else {
            (dispatch.vertex_attrib_i_3ui)(index, x, y, z);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI4ui(index: u32, x: u32, y: u32, z: u32, w: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_4ui)(index, x, y, z, w);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI1iv(index: u32, v: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_1iv as *const ()) {
            (dispatch.vertex_attrib_i_4i)(index, *v, 0, 0, 0);
        } else {
            (dispatch.vertex_attrib_i_1iv)(index, v);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI2iv(index: u32, v: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_2iv as *const ()) {
            (dispatch.vertex_attrib_i_4i)(index, *v, *v.offset(1), 0, 0);
        } else {
            (dispatch.vertex_attrib_i_2iv)(index, v);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI3iv(index: u32, v: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_3iv as *const ()) {
            (dispatch.vertex_attrib_i_4i)(index, *v, *v.offset(1), *v.offset(2), 0);
        } else {
            (dispatch.vertex_attrib_i_3iv)(index, v);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI4iv(index: u32, v: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_4iv)(index, v);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI1uiv(index: u32, v: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_1uiv as *const ()) {
            (dispatch.vertex_attrib_i_4ui)(index, *v, 0, 0, 0);
        } else {
            (dispatch.vertex_attrib_i_1uiv)(index, v);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI2uiv(index: u32, v: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_2uiv as *const ()) {
            (dispatch.vertex_attrib_i_4ui)(index, *v, *v.offset(1), 0, 0);
        } else {
            (dispatch.vertex_attrib_i_2uiv)(index, v);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI3uiv(index: u32, v: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_3uiv as *const ()) {
            (dispatch.vertex_attrib_i_4ui)(index, *v, *v.offset(1), *v.offset(2), 0);
        } else {
            (dispatch.vertex_attrib_i_3uiv)(index, v);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI4uiv(index: u32, v: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_4uiv)(index, v);
    });
}

// === ARB_vertex_attrib_binding / GLES 3.1 DSA API ===
// MC 使用这套 API 替代 glVertexAttribPointer 来设置 VAO 属性。
// glBindVertexBuffer 是其中唯一传递 buffer ID 的函数，需要做 ID 翻译。
// 其余函数直接透传。

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindVertexBuffer(bindingindex: u32, buffer: u32, offset: isize, stride: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.bind_vertex_buffer as *const ()) {
            log::warn!("[FluorateGL] glBindVertexBuffer: stub, ignored");
            return;
        }

        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                    log::warn!(
                        "[FluorateGL] glBindVertexBuffer(binding={}, buffer={}): desktop ID not found in IdMap, unbinding",
                        bindingindex, buffer
                    );
                    0
                })
            })
        };

        log::debug!(
            "[FluorateGL] glBindVertexBuffer(binding={}, buf={}, offset={}, stride={}) desktop {} -> GLES {} (tid={})",
            bindingindex,
            buffer,
            offset,
            stride,
            buffer,
            gles_id,
            state::thread_id_u64()
        );

        (dispatch.bind_vertex_buffer)(bindingindex, gles_id, offset, stride);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribFormat(
    attribindex: u32,
    size: i32,
    type_: u32,
    normalized: u8,
    relativeoffset: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_format as *const ()) {
            log::warn!("[FluorateGL] glVertexAttribFormat: stub, ignored");
            return;
        }
        (dispatch.vertex_attrib_format)(attribindex, size, type_, normalized, relativeoffset);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribIFormat(
    attribindex: u32,
    size: i32,
    type_: u32,
    relativeoffset: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_format as *const ()) {
            log::warn!("[FluorateGL] glVertexAttribIFormat: stub, ignored");
            return;
        }
        (dispatch.vertex_attrib_i_format)(attribindex, size, type_, relativeoffset);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribBinding(attribindex: u32, bindingindex: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_binding as *const ()) {
            log::warn!("[FluorateGL] glVertexAttribBinding: stub, ignored");
            return;
        }
        (dispatch.vertex_attrib_binding)(attribindex, bindingindex);
    });
}
