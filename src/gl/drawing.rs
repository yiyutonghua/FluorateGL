//! Draw call 分发与降级
//!
//! 本模块处理非 Multi 的 draw call。Multi-draw 系列见 [`super::multi_draw`]。
//!
//! 决策依据：优先使用 [`crate::backend::capabilities`]（基于真实 GLES 扩展查询），
//! `is_stub`（函数指针层面）作兜底——即使扩展声明支持，若 `load_opt_suffixes!`
//! 未加载到符号（驱动声明扩展但未导出函数），仍走模拟。
//!
//! 降级策略：
//! - `glDrawRangeElements`：不支持时降级为 `glDrawElements`（start/end 是 hint）
//! - `glPrimitiveRestartIndex`：不支持时静默忽略
//! - BaseVertex 系列：不支持时降级为普通 draw（丢弃 basevertex，影响索引正确性，仅 best-effort）
//! - BaseInstance 系列：不支持时降级为对应 Instanced 版（丢弃 baseinstance）
//! - Indirect 系列：不支持时无法模拟（需读 GPU buffer），告警返回

use crate::backend;
use crate::backend::dispatch::GlesDispatch;
use std::sync::atomic::{AtomicBool, Ordering};

/// BaseVertex 不支持时的首次告警标志（避免每帧刷屏）
static BASE_VERTEX_WARNED: AtomicBool = AtomicBool::new(false);
/// BaseInstance 不支持时的首次告警标志（避免每帧刷屏）
static BASE_INSTANCE_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：BaseVertex 不可用，降级为普通 draw。
fn warn_base_vertex_unsupported(fname: &str) {
    if !BASE_VERTEX_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 不支持 GL_OES_draw_elements_base_vertex，已降级为普通 draw（索引偏移丢失），后续调用将静默降级",
            fname
        );
    }
}

/// 首次告警：BaseInstance 不可用，降级为对应 Instanced 版。
fn warn_base_instance_unsupported(fname: &str) {
    if !BASE_INSTANCE_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 不支持 GL_EXT_base_instance，已降级为对应 Instanced 版（baseinstance 丢失），后续调用将静默降级",
            fname
        );
    }
}

/// 判断 dispatch 函数指针是否为共享的未实现 stub。
///
/// `load_opt!` / `load_opt_suffixes!` 将缺失的可选函数替换为单个 stub 函数，
/// 故所有 stub 字段地址相同。与 `dispatch.stub` 比较即可识别。
fn is_stub(dispatch: &GlesDispatch, ptr: *const ()) -> bool {
    ptr == dispatch.stub as *const ()
}

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
        if is_stub(dispatch, dispatch.draw_range_elements as *const ()) {
            // GLES 不支持 glDrawRangeElements 时降级为 glDrawElements
            // start/end 只是 hint，跳过它们不影响正确性
            log::debug!(
                "[FluorateGL] glDrawRangeElements fallback to glDrawElements (stub detected)"
            );
            (dispatch.draw_elements)(mode, count, type_, indices);
        } else {
            (dispatch.draw_range_elements)(mode, start, end, count, type_, indices);
        }
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
    // glPrimitiveRestartIndex 是 GLES 3.0 core 特性，项目 3.1+ 前提下恒可用。
    // 用 load_opt! 加载仅为兼容旧驱动边界情况，stub 时静默忽略：
    // GLES 默认使用固定重启索引（0xFFFFFFFF for GL_UNSIGNED_INT），
    // MC 一般使用默认值，忽略自定义重启索引不影响正确性。
    backend::with_gles_dispatch(|dispatch| unsafe {
        if !is_stub(dispatch, dispatch.primitive_restart_index as *const ()) {
            (dispatch.primitive_restart_index)(index);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsBaseVertex(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    basevertex: i32,
) {
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 优先用扩展能力判断，is_stub 兜底（驱动声明扩展但未导出符号的边界情况）
        let supported = caps.draw_elements_base_vertex
            && !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
        if !supported {
            // 降级为普通 glDrawElements，丢弃 basevertex 偏移。
            // 注意：索引未偏移会导致顶点错位，仅 best-effort 避免崩溃。
            warn_base_vertex_unsupported("glDrawElementsBaseVertex");
            (dispatch.draw_elements)(mode, count, type_, indices);
        } else {
            (dispatch.draw_elements_base_vertex)(mode, count, type_, indices, basevertex);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysIndirect(mode: u32, indirect: *const std::ffi::c_void) {
    // GLES 3.1 core 特性，项目前提，直接转发
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_arrays_indirect)(mode, indirect);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsIndirect(mode: u32, type_: u32, indirect: *const std::ffi::c_void) {
    // GLES 3.1 core 特性，项目前提，直接转发
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_elements_indirect)(mode, type_, indirect);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysInstancedBaseInstance(
    mode: u32,
    first: i32,
    count: i32,
    instancecount: i32,
    baseinstance: u32,
) {
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.base_instance
            && !is_stub(
                dispatch,
                dispatch.draw_arrays_instanced_base_instance as *const (),
            );
        if !supported {
            // 降级为 glDrawArraysInstanced，丢弃 baseinstance。
            // 影响：使用 instance ID 计算属性偏移的 shader 会错位，仅 best-effort。
            warn_base_instance_unsupported("glDrawArraysInstancedBaseInstance");
            (dispatch.draw_arrays_instanced)(mode, first, count, instancecount);
        } else {
            (dispatch.draw_arrays_instanced_base_instance)(
                mode,
                first,
                count,
                instancecount,
                baseinstance,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsInstancedBaseInstance(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    instancecount: i32,
    baseinstance: u32,
) {
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.base_instance
            && !is_stub(
                dispatch,
                dispatch.draw_elements_instanced_base_instance as *const (),
            );
        if !supported {
            warn_base_instance_unsupported("glDrawElementsInstancedBaseInstance");
            (dispatch.draw_elements_instanced)(mode, count, type_, indices, instancecount);
        } else {
            (dispatch.draw_elements_instanced_base_instance)(
                mode,
                count,
                type_,
                indices,
                instancecount,
                baseinstance,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsInstancedBaseVertex(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    instancecount: i32,
    basevertex: i32,
) {
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.draw_elements_base_vertex
            && !is_stub(
                dispatch,
                dispatch.draw_elements_instanced_base_vertex as *const (),
            );
        if !supported {
            warn_base_vertex_unsupported("glDrawElementsInstancedBaseVertex");
            (dispatch.draw_elements_instanced)(mode, count, type_, indices, instancecount);
        } else {
            (dispatch.draw_elements_instanced_base_vertex)(
                mode,
                count,
                type_,
                indices,
                instancecount,
                basevertex,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsInstancedBaseVertexBaseInstance(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    instancecount: i32,
    basevertex: i32,
    baseinstance: u32,
) {
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 需要 base_vertex 和 base_instance 都支持
        let supported = caps.draw_elements_base_vertex
            && caps.base_instance
            && !is_stub(
                dispatch,
                dispatch.draw_elements_instanced_base_vertex_base_instance as *const (),
            );
        if !supported {
            // 同时丢失 basevertex 和 baseinstance，触发两类首次告警
            warn_base_vertex_unsupported("glDrawElementsInstancedBaseVertexBaseInstance");
            warn_base_instance_unsupported("glDrawElementsInstancedBaseVertexBaseInstance");
            (dispatch.draw_elements_instanced)(mode, count, type_, indices, instancecount);
        } else {
            (dispatch.draw_elements_instanced_base_vertex_base_instance)(
                mode,
                count,
                type_,
                indices,
                instancecount,
                basevertex,
                baseinstance,
            );
        }
    });
}
