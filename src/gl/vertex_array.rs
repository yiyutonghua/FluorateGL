use crate::backend;
use crate::state;
use std::sync::atomic::{AtomicBool, Ordering};

/// VAO 相关资源 desktop ID 查找失败首次告警标志
/// glBindVertexArray：VAO ID 未在 IdMap 中找到
static VAO_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);
/// glBindVertexBuffer：buffer ID 未在 IdMap 中找到
static VERTEX_BUFFER_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);
/// ARB_vertex_attrib_binding 系列（GLES 3.1 core）函数为 stub 时的首次告警标志
/// 覆盖：glBindVertexBuffer / glVertexAttribFormat / glVertexAttribIFormat / glVertexAttribBinding
static VERTEX_ATTRIB_BINDING_STUB_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glBindVertexArray 的 VAO desktop ID 未在 IdMap 中找到。
fn warn_vao_id_miss(array: u32) {
    if !VAO_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glBindVertexArray({}): desktop ID not found in IdMap, unbinding (跨线程或资源已释放，后续将静默降级)",
            array
        );
    }
}

/// 首次告警：glBindVertexBuffer 的 buffer desktop ID 未在 IdMap 中找到。
fn warn_vertex_buffer_id_miss(bindingindex: u32, buffer: u32) {
    if !VERTEX_BUFFER_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glBindVertexBuffer(binding={}, buffer={}): desktop ID not found in IdMap, unbinding (跨线程或资源已释放，后续将静默降级)",
            bindingindex,
            buffer
        );
    }
}

/// 首次告警：ARB_vertex_attrib_binding 系列函数为 stub，已忽略。
///
/// 这些函数是 GLES 3.1 core 特性（项目前提），stub 表示驱动未导出符号，
/// 属于驱动边界情况。后续调用将静默跳过。
fn warn_vertex_attrib_binding_stub(fname: &str) {
    if !VERTEX_ATTRIB_BINDING_STUB_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 3.1 ARB_vertex_attrib_binding 函数为 stub，已忽略 (驱动边界情况，后续将静默跳过)",
            fname
        );
    }
}

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
                    warn_vao_id_miss(array);
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
    log::debug!(
        "[FluorateGL] glVertexAttribIPointer(index={}, size={}, type=0x{:04X}, stride={})",
        index,
        size,
        type_,
        stride
    );
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

// A. ARB 别名：转发到 core 版本

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribDivisorARB(index: u32, divisor: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_divisor)(index, divisor);
    });
}

// B. VertexAttrib short/double 版本：转换为 float 后调用对应的 float 版本

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib1s(index: u32, x: i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_1f)(index, x as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib2s(index: u32, x: i16, y: i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_2f)(index, x as f32, y as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib3s(index: u32, x: i16, y: i16, z: i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_3f)(index, x as f32, y as f32, z as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4s(index: u32, x: i16, y: i16, z: i16, w: i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(index, x as f32, y as f32, z as f32, w as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib1d(index: u32, x: f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_1f)(index, x as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib2d(index: u32, x: f64, y: f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_2f)(index, x as f32, y as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib3d(index: u32, x: f64, y: f64, z: f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_3f)(index, x as f32, y as f32, z as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4d(index: u32, x: f64, y: f64, z: f64, w: f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(index, x as f32, y as f32, z as f32, w as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib1sv(index: u32, v: *const i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_1f)(index, *v as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib2sv(index: u32, v: *const i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_2f)(index, *v as f32, *v.offset(1) as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib3sv(index: u32, v: *const i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_3f)(index, *v as f32, *v.offset(1) as f32, *v.offset(2) as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4sv(index: u32, v: *const i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32,
            *v.offset(1) as f32,
            *v.offset(2) as f32,
            *v.offset(3) as f32,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib1dv(index: u32, v: *const f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_1f)(index, *v as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib2dv(index: u32, v: *const f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_2f)(index, *v as f32, *v.offset(1) as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib3dv(index: u32, v: *const f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_3f)(index, *v as f32, *v.offset(1) as f32, *v.offset(2) as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4dv(index: u32, v: *const f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32,
            *v.offset(1) as f32,
            *v.offset(2) as f32,
            *v.offset(3) as f32,
        );
    });
}

// C. VertexAttrib4 整数向量版本：转换为 float 后调用 vertex_attrib_4f

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4iv(index: u32, v: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32,
            *v.offset(1) as f32,
            *v.offset(2) as f32,
            *v.offset(3) as f32,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4bv(index: u32, v: *const i8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32,
            *v.offset(1) as f32,
            *v.offset(2) as f32,
            *v.offset(3) as f32,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4ubv(index: u32, v: *const u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32,
            *v.offset(1) as f32,
            *v.offset(2) as f32,
            *v.offset(3) as f32,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4usv(index: u32, v: *const u16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32,
            *v.offset(1) as f32,
            *v.offset(2) as f32,
            *v.offset(3) as f32,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4uiv(index: u32, v: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32,
            *v.offset(1) as f32,
            *v.offset(2) as f32,
            *v.offset(3) as f32,
        );
    });
}

// D. VertexAttrib4N normalized 版本：将整数归一化到 [0,1] 或 [-1,1] 浮点范围后调用 vertex_attrib_4f

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4Nub(index: u32, x: u8, y: u8, z: u8, w: u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            x as f32 / 255.0,
            y as f32 / 255.0,
            z as f32 / 255.0,
            w as f32 / 255.0,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4Nbv(index: u32, v: *const i8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32 / 127.0,
            *v.offset(1) as f32 / 127.0,
            *v.offset(2) as f32 / 127.0,
            *v.offset(3) as f32 / 127.0,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4Nsv(index: u32, v: *const i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32 / 32767.0,
            *v.offset(1) as f32 / 32767.0,
            *v.offset(2) as f32 / 32767.0,
            *v.offset(3) as f32 / 32767.0,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4Niv(index: u32, v: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32 / 2147483647.0,
            *v.offset(1) as f32 / 2147483647.0,
            *v.offset(2) as f32 / 2147483647.0,
            *v.offset(3) as f32 / 2147483647.0,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4Nubv(index: u32, v: *const u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32 / 255.0,
            *v.offset(1) as f32 / 255.0,
            *v.offset(2) as f32 / 255.0,
            *v.offset(3) as f32 / 255.0,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4Nusv(index: u32, v: *const u16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32 / 65535.0,
            *v.offset(1) as f32 / 65535.0,
            *v.offset(2) as f32 / 65535.0,
            *v.offset(3) as f32 / 65535.0,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttrib4Nuiv(index: u32, v: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_4f)(
            index,
            *v as f32 / 4294967295.0,
            *v.offset(1) as f32 / 4294967295.0,
            *v.offset(2) as f32 / 4294967295.0,
            *v.offset(3) as f32 / 4294967295.0,
        );
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

// E. VertexAttribI4 整数向量版本：转换为 i32/u32 后调用 vertex_attrib_i_4i 或 vertex_attrib_i_4ui

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI4bv(index: u32, v: *const i8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_4i)(
            index,
            *v as i32,
            *v.offset(1) as i32,
            *v.offset(2) as i32,
            *v.offset(3) as i32,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI4sv(index: u32, v: *const i16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_4i)(
            index,
            *v as i32,
            *v.offset(1) as i32,
            *v.offset(2) as i32,
            *v.offset(3) as i32,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI4ubv(index: u32, v: *const u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_4ui)(
            index,
            *v as u32,
            *v.offset(1) as u32,
            *v.offset(2) as u32,
            *v.offset(3) as u32,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribI4usv(index: u32, v: *const u16) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.vertex_attrib_i_4ui)(
            index,
            *v as u32,
            *v.offset(1) as u32,
            *v.offset(2) as u32,
            *v.offset(3) as u32,
        );
    });
}

// MC 使用 ARB_vertex_attrib_binding / GLES 3.1 DSA API 替代 glVertexAttribPointer 来设置 VAO 属性。
// glBindVertexBuffer 是其中唯一传递 buffer ID 的函数，需要做 ID 翻译。
// 其余函数直接透传。

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindVertexBuffer(bindingindex: u32, buffer: u32, offset: isize, stride: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.bind_vertex_buffer as *const ()) {
            warn_vertex_attrib_binding_stub("glBindVertexBuffer");
            return;
        }

        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                    warn_vertex_buffer_id_miss(bindingindex, buffer);
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
    log::debug!(
        "[FluorateGL] glVertexAttribFormat(attrib={}, size={}, type=0x{:04X}, normalized={}, offset={})",
        attribindex,
        size,
        type_,
        normalized,
        relativeoffset
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_format as *const ()) {
            warn_vertex_attrib_binding_stub("glVertexAttribFormat");
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
    log::debug!(
        "[FluorateGL] glVertexAttribIFormat(attrib={}, size={}, type=0x{:04X}, offset={})",
        attribindex,
        size,
        type_,
        relativeoffset
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_i_format as *const ()) {
            warn_vertex_attrib_binding_stub("glVertexAttribIFormat");
            return;
        }
        (dispatch.vertex_attrib_i_format)(attribindex, size, type_, relativeoffset);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glVertexAttribBinding(attribindex: u32, bindingindex: u32) {
    log::debug!(
        "[FluorateGL] glVertexAttribBinding(attrib={}, binding={})",
        attribindex,
        bindingindex
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.vertex_attrib_binding as *const ()) {
            warn_vertex_attrib_binding_stub("glVertexAttribBinding");
            return;
        }
        (dispatch.vertex_attrib_binding)(attribindex, bindingindex);
    });
}
