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

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFloatv(pname: u32, data: *mut f32) {
    if data.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_float_v)(pname, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetDoublev(pname: u32, data: *mut f64) {
    if data.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_double_v)(pname, data);
    });
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
        0x90D3 // GL_SHADER_STORAGE_BUFFER_BINDING
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

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetDoublei_v(target: u32, index: u32, data: *mut f64) {
    if data.is_null() {
        return;
    }
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
