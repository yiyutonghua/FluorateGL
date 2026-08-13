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

/// glGetBooleanv — 移植 MobileGlues enable.cpp 语义：
/// enable 表优先（cap 状态 → 0/1、表内 int 状态 → 非 0 即 1），
/// 再 pixel store 影子表，最后透传驱动。与 glIsEnabled 永远一致。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBooleanv(pname: u32, data: *mut u8) {
    if data.is_null() {
        return;
    }
    let mut enabled = 0u8;
    if crate::gl::exports::enable_state::mg_enable_query(pname, &mut enabled) {
        unsafe { *data = enabled };
        return;
    }
    let mut ival = 0i32;
    if crate::gl::exports::enable_state::mg_enable_query_int(pname, &mut ival) {
        unsafe { *data = if ival != 0 { 1 } else { 0 } };
        return;
    }
    if crate::gl::pixel::pixel_store::query_int(pname, &mut ival) {
        unsafe { *data = if ival != 0 { 1 } else { 0 } };
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

/// glGetFloatv — 移植 MobileGlues enable.cpp 语义（enable 表 → 表内 int 状态
/// → 我们保留的 line-width 拦截 → pixel store 影子表 → 透传驱动）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFloatv(pname: u32, data: *mut f32) {
    if data.is_null() {
        return;
    }
    let mut enabled = 0u8;
    if crate::gl::exports::enable_state::mg_enable_query(pname, &mut enabled) {
        unsafe { *data = if enabled != 0 { 1.0 } else { 0.0 } };
        return;
    }
    let mut ival = 0i32;
    if crate::gl::exports::enable_state::mg_enable_query_int(pname, &mut ival) {
        unsafe { *data = ival as f32 };
        return;
    }
    if fill_line_width_query(pname, data) {
        return;
    }
    if crate::gl::pixel::pixel_store::query_int(pname, &mut ival) {
        unsafe { *data = ival as f32 };
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_float_v)(pname, data);
    });
}

/// glGetFloatv 为各 pname 写入的分量数（移植 MG enable.cpp:642-659
/// mg_get_components：GL_DEPTH_RANGE 写 2、GL_VIEWPORT 写 4，其余写 1。
/// GLES 驱动收到比 4 更大的输出缓冲也无妨，但读回后逐元素扩展必须
/// 按此表限制写回数量，避免越界写调用方缓冲）。
fn get_float_components(pname: u32) -> usize {
    match pname {
        0x0B70 | // GL_DEPTH_RANGE
        0x846E | // GL_ALIASED_LINE_WIDTH_RANGE
        0x846D | // GL_ALIASED_POINT_SIZE_RANGE
        0x0D3A | // GL_MAX_VIEWPORT_DIMS
        0x0B12 // GL_POINT_SIZE_RANGE
        => 2,
        0x0BA2 | // GL_VIEWPORT
        0x0C10 | // GL_SCISSOR_BOX
        0x0C22 | // GL_COLOR_CLEAR_VALUE
        0x8005 | // GL_BLEND_COLOR
        0x0C23 // GL_COLOR_WRITEMASK
        => 4,
        _ => 1,
    }
}

/// glGetDoublev — GLES 无 double 返回查询函数（dispatch.get_double_v 加载必失败，
/// 原直通为 no-op stub 且不写 data → 宿主读垃圾值）。
///
/// 移植 MG enable.cpp glGetDoublev：enable 表优先（bool/int），pixel store
/// 影子表，其余经 glGetFloatv 查询后按 mg_get_components 分量数扩展为 f64
/// （桌面 double 查询如 GL_DEPTH_RANGE 最多写 2 个、GL_VIEWPORT 写 4 个；
/// pname 非法时 GLES 不写 temp，temp 预填 0 保证调用方至少读到 0）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetDoublev(pname: u32, data: *mut f64) {
    if data.is_null() {
        return;
    }
    let mut enabled = 0u8;
    if crate::gl::exports::enable_state::mg_enable_query(pname, &mut enabled) {
        unsafe { *data = if enabled != 0 { 1.0 } else { 0.0 } };
        return;
    }
    let mut ival = 0i32;
    if crate::gl::exports::enable_state::mg_enable_query_int(pname, &mut ival) {
        unsafe { *data = ival as f64 };
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
    if crate::gl::pixel::pixel_store::query_int(pname, &mut ival) {
        unsafe { *data = ival as f64 };
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_float_v)(pname, temp.as_mut_ptr());
    });
    unsafe {
        let n = get_float_components(pname);
        for (i, v) in temp.iter().take(n).enumerate() {
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

/// glGetInteger64v — 移植 MG enable.cpp 语义（enable 表 → pixel store
/// 影子表 → 透传驱动）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetInteger64v(pname: u32, data: *mut i64) {
    if data.is_null() {
        return;
    }
    let mut enabled = 0u8;
    if crate::gl::exports::enable_state::mg_enable_query(pname, &mut enabled) {
        unsafe { *data = enabled as i64 };
        return;
    }
    let mut ival = 0i32;
    if crate::gl::exports::enable_state::mg_enable_query_int(pname, &mut ival) {
        unsafe { *data = ival as i64 };
        return;
    }
    if crate::gl::pixel::pixel_store::query_int(pname, &mut ival) {
        unsafe { *data = ival as i64 };
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

/// glIsEnabled — 虚拟 enable 表回答（移植 MobileGlues enable.cpp）。
///
/// 语义变化说明：旧实现为透传驱动 + is_unsupported_gles_cap 过滤；现改为
/// enable_state 表回答（MG 语义）——所有 enable 能力状态由表持有，
/// glEnable/glDisable 写表、glIsEnabled 读表，驱动不参与回答。
/// 未知/非法 cap 返回 GL_FALSE（GL 规范对错误枚举的答案）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsEnabled(cap: u32) -> u8 {
    crate::gl::exports::enable_state::mg_enable_get(cap, 0)
}

/// glIsEnabledi — 与 glIsEnabled 同表（BLEND 按 draw buffer、SCISSOR_TEST
/// 按 viewport 索引回答，其余 cap 忽略 index）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsEnabledi(cap: u32, index: u32) -> u8 {
    crate::gl::exports::enable_state::mg_enable_get(cap, index)
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

// ==== GL 2.0-3.3 core 查询类补齐（GLES 原生透传 / 模拟）====

/// glIsVertexArray — GL 3.0 core（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsVertexArray(array: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.is_vertex_array)(array) })
}

/// glGetVertexAttribiv — GL 2.0（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetVertexAttribiv(index: u32, pname: u32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_vertex_attrib_iv)(index, pname, params);
    });
}

/// glGetVertexAttribIiv — GL 3.0（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetVertexAttribIiv(index: u32, pname: u32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_vertex_attrib_i_iv)(index, pname, params);
    });
}

/// glGetVertexAttribIuiv — GL 3.0（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetVertexAttribIuiv(index: u32, pname: u32, params: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_vertex_attrib_i_uiv)(index, pname, params);
    });
}

/// glGetVertexAttribPointerv — GL 2.0（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetVertexAttribPointerv(
    index: u32,
    pname: u32,
    pointer: *mut *mut std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_vertex_attrib_pointer_v)(index, pname, pointer);
    });
}

/// glGetTexParameterfv — GL 1.1（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTexParameterfv(target: u32, pname: u32, params: *mut f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_tex_parameter_fv)(target, pname, params);
    });
}

/// glGetTexParameterIiv — GL 3.0（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTexParameterIiv(target: u32, pname: u32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_tex_parameter_i_iv)(target, pname, params);
    });
}

/// glGetTexParameterIuiv — GL 3.0（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTexParameterIuiv(target: u32, pname: u32, params: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_tex_parameter_i_uiv)(target, pname, params);
    });
}

/// glGetTexLevelParameterfv — GL 1.2（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTexLevelParameterfv(target: u32, level: i32, pname: u32, params: *mut f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_tex_level_parameter_fv)(target, level, pname, params);
    });
}

/// glGetInternalformativ — GL 4.2（GLES 3.0 原生透传；GL 4.2 语义与 GLES 相同）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetInternalformativ(
    target: u32,
    internalformat: u32,
    pname: u32,
    buf_size: i32,
    params: *mut i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_internalformat_iv)(target, internalformat, pname, buf_size, params);
    });
}

/// glGetFramebufferParameteriv — GL 4.3（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFramebufferParameteriv(target: u32, pname: u32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_framebuffer_parameter_iv)(target, pname, params);
    });
}

/// glGetRenderbufferParameteriv — GL 3.0（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetRenderbufferParameteriv(target: u32, pname: u32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_renderbuffer_parameter_iv)(target, pname, params);
    });
}

/// glGetMultisamplefv — GL 3.2（GLES 3.1 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetMultisamplefv(pname: u32, index: u32, val: *mut f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_multisample_fv)(pname, index, val);
    });
}

/// glGetBufferParameteri64v — GL 3.2（GLES 3.2 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferParameteri64v(target: u32, pname: u32, params: *mut i64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_buffer_parameter_i64v)(target, pname, params);
    });
}

/// glGetPointerv — GL 1.1（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetPointerv(pname: u32, params: *mut *mut std::ffi::c_void) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_pointer_v)(pname, params);
    });
}

/// glSampleCoverage — GL 1.3（GLES 3.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glSampleCoverage(value: f32, invert: u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.sample_coverage)(value, invert);
    });
}

/// glSampleMaski — GL 3.2（GLES 3.1 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glSampleMaski(mask_number: u32, mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.sample_mask_i)(mask_number, mask);
    });
}

/// glBlendColor — GL 1.4（GLES 2.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendColor(red: f32, green: f32, blue: f32, alpha: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_color)(red, green, blue, alpha);
    });
}

/// glPointParameterf — GL 1.4（GLES 2.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPointParameterf(pname: u32, param: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.point_parameter_f)(pname, param);
    });
}

/// glPointParameterfv — GL 1.4（GLES 2.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPointParameterfv(pname: u32, params: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.point_parameter_fv)(pname, params);
    });
}

/// glHint — GL 1.0（GLES 2.0 原生透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glHint(target: u32, mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.hint)(target, mode);
    });
}

/// glGetInternalformati64v — GL 4.2（GLES 无对应，stub 返回 0）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetInternalformati64v(
    _target: u32,
    _internalformat: u32,
    _pname: u32,
    _buf_size: i32,
    _params: *mut i64,
) {
    log::debug!("[FluorateGL] glGetInternalformati64v stub (GLES 无对应)");
}

/// glGetQueryIndexediv — GL 4.0（GLES 无对应，stub no-op）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetQueryIndexediv(_target: u32, _index: u32, _pname: u32, _params: *mut i32) {
    log::debug!("[FluorateGL] glGetQueryIndexediv stub (GLES 无对应)");
}
