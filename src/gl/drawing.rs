//! Draw call 分发与降级
//!
//! 本模块处理非 Multi 的 draw call。Multi-draw 系列见 [`super::multi_draw`]。
//!
//! 降级策略：
//! - `glDrawRangeElements`：stub 时降级为 `glDrawElements`（start/end 是 hint，可丢弃）
//! - `glPrimitiveRestartIndex`：stub 时静默忽略
//! - BaseVertex 系列：stub 时降级为普通 draw（丢弃 basevertex 偏移，影响索引正确性，仅 best-effort）
//! - BaseInstance 系列：stub 时降级为对应 Instanced 版（丢弃 baseinstance）
//! - Indirect 系列：stub 时无法模拟（需读 GPU buffer），告警返回

use crate::backend;
use crate::backend::dispatch::GlesDispatch;

/// 判断 dispatch 函数指针是否为共享的未实现 stub。
///
/// `load_opt!` substitutes missing optional functions with a single stub function,
/// so every stub field in `GlesDispatch` has the same address. We compare against
/// `dispatch.stub`, which is initialized to that exact stub address.
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        if !is_stub(dispatch, dispatch.primitive_restart_index as *const ()) {
            (dispatch.primitive_restart_index)(index);
        }
        // GLES 不支持此函数时静默忽略
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ()) {
            // 降级为普通 glDrawElements，丢弃 basevertex 偏移。
            // 注意：索引未偏移会导致顶点错位，仅 best-effort 避免崩溃。
            log::warn!(
                "[FluorateGL] glDrawElementsBaseVertex: GLES 不支持 basevertex，已降级为 glDrawElements（索引偏移丢失）"
            );
            (dispatch.draw_elements)(mode, count, type_, indices);
        } else {
            (dispatch.draw_elements_base_vertex)(mode, count, type_, indices, basevertex);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysIndirect(mode: u32, indirect: *const std::ffi::c_void) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.draw_arrays_indirect as *const ()) {
            // indirect draw 需从 GPU buffer 读取 command，CPU 侧无法安全模拟。
            log::warn!(
                "[FluorateGL] glDrawArraysIndirect: GLES 不支持 indirect draw，无法模拟，已跳过"
            );
            return;
        }
        (dispatch.draw_arrays_indirect)(mode, indirect);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsIndirect(mode: u32, type_: u32, indirect: *const std::ffi::c_void) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.draw_elements_indirect as *const ()) {
            log::warn!(
                "[FluorateGL] glDrawElementsIndirect: GLES 不支持 indirect draw，无法模拟，已跳过"
            );
            return;
        }
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(
            dispatch,
            dispatch.draw_arrays_instanced_base_instance as *const (),
        ) {
            // 降级为 glDrawArraysInstanced，丢弃 baseinstance。
            // 影响：使用 instance ID 计算属性偏移的 shader 会错位，仅 best-effort。
            log::warn!(
                "[FluorateGL] glDrawArraysInstancedBaseInstance: GLES 不支持 baseinstance，已降级为 glDrawArraysInstanced（baseinstance 丢失）"
            );
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(
            dispatch,
            dispatch.draw_elements_instanced_base_instance as *const (),
        ) {
            log::warn!(
                "[FluorateGL] glDrawElementsInstancedBaseInstance: GLES 不支持 baseinstance，已降级为 glDrawElementsInstanced（baseinstance 丢失）"
            );
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(
            dispatch,
            dispatch.draw_elements_instanced_base_vertex as *const (),
        ) {
            log::warn!(
                "[FluorateGL] glDrawElementsInstancedBaseVertex: GLES 不支持 basevertex，已降级为 glDrawElementsInstanced（索引偏移丢失）"
            );
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(
            dispatch,
            dispatch.draw_elements_instanced_base_vertex_base_instance as *const (),
        ) {
            log::warn!(
                "[FluorateGL] glDrawElementsInstancedBaseVertexBaseInstance: GLES 不支持 basevertex/baseinstance，已降级为 glDrawElementsInstanced（索引偏移与 baseinstance 丢失）"
            );
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
