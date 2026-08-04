use crate::backend;
use crate::state;
use std::sync::atomic::{AtomicBool, Ordering};

/// glGetIntegeri_v 索引绑定查询时 GLES ID 未在 IdMap 中找到首次告警标志
static INDEXED_BINDING_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glGetIntegeri_v 索引绑定查询 GLES ID 未在 IdMap 中找到。
fn warn_indexed_binding_id_miss(target: u32, gles_id: u32) {
    if !INDEXED_BINDING_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glGetIntegeri_v(0x{:04X}): GLES ID {} not found in IdMap, returning raw GLES ID (跨线程或资源已释放，后续将静默返回原始 GLES ID)",
            target,
            gles_id
        );
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBooleanv(pname: u32, data: *mut u8) {
    if data.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_boolean_v)(pname, data);
    });
}

/// 填充 GL_LINE_WIDTH_RANGE / GL_LINE_WIDTH_GRANULARITY 查询（GLES 无此 pname，
/// 直通会产生 INVALID_ENUM 且不写 data，宿主读到垃圾值）。
///
/// - 0x0B22 (GL_LINE_WIDTH_RANGE) → 用 GLES 的 GL_ALIASED_LINE_WIDTH_RANGE (0x846E)
///   查询值填充（GLES 3.0 core；llvmpipe 实测 [1,255] 与桌面一致）。
/// - 0x0B23 (GL_LINE_WIDTH_GRANULARITY) → GLES 无对应 pname，填 0 表示无步进限制
///   （GLES 实际接受任意线宽并钳制到 ALIASED 范围，0 为保守语义）。
///
/// 返回 true 表示已拦截处理（调用方应返回），false 表示非本类 pname 需直通。
fn fill_line_width_query(pname: u32, data_f32: *mut f32) -> bool {
    match pname {
        0x0B22 => {
            let mut r = [0.0f32; 2];
            backend::with_gles_dispatch(|dispatch| unsafe {
                (dispatch.get_float_v)(0x846E, r.as_mut_ptr());
            });
            unsafe {
                *data_f32 = r[0];
                *data_f32.add(1) = r[1];
            }
            true
        }
        0x0B23 => {
            unsafe { *data_f32 = 0.0 };
            true
        }
        _ => false,
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFloatv(pname: u32, data: *mut f32) {
    if data.is_null() {
        return;
    }
    if fill_line_width_query(pname, data) {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_float_v)(pname, data);
    });
}

/// glGetDoublev — GLES 无 double 返回查询函数（dispatch.get_double_v 加载必失败，
/// 原直通为 no-op stub 且不写 data → 宿主读垃圾值）。
///
/// 改为经 glGetFloatv 查询后逐元素扩展为 f64（与 glGetVertexAttribdv 同模式）。
/// 桌面 double 查询（GL_DEPTH_RANGE、GL_COLOR_CLEAR_VALUE 等）最多返回 4 个分量；
/// pname 非法时 GLES 不写 temp，temp 预填 0 保证调用方至少读到 0。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetDoublev(pname: u32, data: *mut f64) {
    if data.is_null() {
        return;
    }
    let mut temp = [0.0f32; 4];
    if fill_line_width_query(pname, temp.as_mut_ptr()) {
        unsafe {
            for (i, v) in temp.iter().enumerate() {
                *data.add(i) = *v as f64;
            }
        }
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_float_v)(pname, temp.as_mut_ptr());
    });
    unsafe {
        for (i, v) in temp.iter().enumerate() {
            *data.add(i) = *v as f64;
        }
    }
}

/// `glGetIntegerv` 的透传版本，供 exports.rs 中特殊处理回退时调用，
/// 避免在 getter.rs 与 exports.rs 中重复导出 C 符号。
pub fn get_integerv(pname: u32, data: *mut i32) {
    if data.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_integerv)(pname, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetInteger64v(pname: u32, data: *mut i64) {
    if data.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_integer_64v)(pname, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBooleani_v(target: u32, index: u32, data: *mut u8) {
    if data.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_booleani_v)(target, index, data);
    });
}

/// 检查 target 是否为索引绑定查询，若是则将 GLES ID 翻译为桌面 ID。
fn translate_indexed_binding_to_desktop(target: u32, data: *mut i32) {
    let gles_id = unsafe { *data } as u32;
    if gles_id == 0 {
        return;
    }

    let desktop_id = match target {
        // 索引 Buffer 绑定查询 → buffers IdMap
        0x8C8F | // GL_TRANSFORM_FEEDBACK_BUFFER_BINDING
        0x8A28 | // GL_UNIFORM_BUFFER_BINDING
        0x90D3 | // GL_SHADER_STORAGE_BUFFER_BINDING
        0x92C1 // GL_ATOMIC_COUNTER_BUFFER_BINDING（GLES 3.1）
        => {
            state::with_state(|s| s.buffers.get_desktop(gles_id))
        }
        _ => return, // 不是绑定查询，无需翻译
    };

    if let Some(desktop_id) = desktop_id {
        if desktop_id != gles_id {
            unsafe { *data = desktop_id as i32 };
        }
    } else {
        warn_indexed_binding_id_miss(target, gles_id);
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetIntegeri_v(target: u32, index: u32, data: *mut i32) {
    if data.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_integeri_v)(target, index, data);
    });
    translate_indexed_binding_to_desktop(target, data);
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFloati_v(target: u32, index: u32, data: *mut f32) {
    if data.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_floati_v)(target, index, data);
    });
}

/// glGetDoublei_v — GLES 无此函数（dispatch.get_doublei_v 加载必失败，同 glGetDoublev），
/// 经 glGetFloati_v 查询后扩展为 f64。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetDoublei_v(target: u32, index: u32, data: *mut f64) {
    if data.is_null() {
        return;
    }
    let mut temp = [0.0f32; 4];
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_floati_v)(target, index, temp.as_mut_ptr());
    });
    unsafe {
        for (i, v) in temp.iter().enumerate() {
            *data.add(i) = *v as f64;
        }
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsEnabled(cap: u32) -> u8 {
    // GLES 无此 cap（或恒开启）时直通会 INVALID_ENUM：按 GLES 对无效 cap 的
    // 语义返回 GL_FALSE（与 glEnable/glDisable 的过滤对称，见 exports.rs）。
    if crate::gl::exports::is_unsupported_gles_cap(cap) {
        return 0;
    }
    // 与 glEnable 对称翻译（GL_PRIMITIVE_RESTART 0x8F9D → GLES 3.0
    // GL_PRIMITIVE_RESTART_FIXED_INDEX 0x8D69，否则查询结果与启用状态错位）。
    let cap = crate::gl::exports::translate_enable_cap(cap);
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.is_enabled)(cap) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsEnabledi(cap: u32, index: u32) -> u8 {
    if crate::gl::exports::is_unsupported_gles_cap(cap) {
        return 0;
    }
    let cap = crate::gl::exports::translate_enable_cap(cap);
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.is_enabled_i)(cap, index) })
}

/// glGetVertexAttribdv — GL 2.0 顶点属性查询（double 数组版本）。
///
/// GLES 仅提供 glGetVertexAttribfv（float 版本），故分配临时 f32 缓冲接收结果，
/// 再逐元素扩展为 f64 写入调用方缓冲。vertex attrib 查询最多返回 4 个分量
/// （如 GL_CURRENT_VERTEX_ATTRIB）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetVertexAttribdv(index: u32, pname: u32, params: *mut f64) {
    if params.is_null() {
        return;
    }
    let mut temp = [0.0f32; 4];
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_vertex_attrib_fv)(index, pname, temp.as_mut_ptr());
    });
    unsafe {
        for (i, v) in temp.iter().enumerate() {
            *params.add(i) = *v as f64;
        }
    }
}
