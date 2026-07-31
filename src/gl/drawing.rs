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
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 优先用扩展能力判断，is_stub 兜底（驱动声明扩展但未导出符号的边界情况）
        let supported = caps.draw_elements_base_vertex
            && !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
        if !supported {
            // 降级为普通 glDrawElements，丢弃 basevertex 偏移。
            // 注意：索引未偏移会导致顶点错位，仅 best-effort 避免崩溃。
            log::warn!(
                "[FluorateGL] glDrawElementsBaseVertex: GLES 不支持 GL_OES_draw_elements_base_vertex，已降级为 glDrawElements（索引偏移丢失）"
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
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported =
            caps.indirect_draw && !is_stub(dispatch, dispatch.draw_arrays_indirect as *const ());
        if !supported {
            // indirect draw 需从 GPU buffer 读取 command，CPU 侧无法安全模拟。
            log::warn!(
                "[FluorateGL] glDrawArraysIndirect: GLES 不支持 indirect draw（需 GLES 3.1+），无法模拟，已跳过"
            );
            return;
        }
        (dispatch.draw_arrays_indirect)(mode, indirect);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsIndirect(mode: u32, type_: u32, indirect: *const std::ffi::c_void) {
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported =
            caps.indirect_draw && !is_stub(dispatch, dispatch.draw_elements_indirect as *const ());
        if !supported {
            log::warn!(
                "[FluorateGL] glDrawElementsIndirect: GLES 不支持 indirect draw（需 GLES 3.1+），无法模拟，已跳过"
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
            log::warn!(
                "[FluorateGL] glDrawArraysInstancedBaseInstance: GLES 不支持 GL_EXT_base_instance，已降级为 glDrawArraysInstanced（baseinstance 丢失）"
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
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.base_instance
            && !is_stub(
                dispatch,
                dispatch.draw_elements_instanced_base_instance as *const (),
            );
        if !supported {
            log::warn!(
                "[FluorateGL] glDrawElementsInstancedBaseInstance: GLES 不支持 GL_EXT_base_instance，已降级为 glDrawElementsInstanced（baseinstance 丢失）"
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
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.draw_elements_base_vertex
            && !is_stub(
                dispatch,
                dispatch.draw_elements_instanced_base_vertex as *const (),
            );
        if !supported {
            log::warn!(
                "[FluorateGL] glDrawElementsInstancedBaseVertex: GLES 不支持 GL_OES_draw_elements_base_vertex，已降级为 glDrawElementsInstanced（索引偏移丢失）"
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
