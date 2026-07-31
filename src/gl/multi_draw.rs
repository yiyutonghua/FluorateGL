//! Multi-draw 系列函数拦截层
//!
//! 桌面 GL 的 `glMultiDraw*` 系列在一次调用中提交多个 draw command，减少 CPU 往返。
//!
//! 决策依据：优先使用 [`crate::backend::capabilities`]（基于真实 GLES 扩展查询），
//! `is_stub`（函数指针层面）作兜底——即使扩展声明支持，若 `load_opt_suffixes!`
//! 未加载到符号（驱动声明扩展但未导出函数），仍走模拟。
//!
//! stub 降级策略：
//! - `glMultiDrawArrays/Elements`：循环调用对应的单次 draw
//! - `glMultiDrawElementsBaseVertex`：优先循环 `glDrawElementsBaseVertex`（保留 basevertex），
//!   否则循环 `glDrawElements`（丢弃 basevertex）
//! - `glMultiDrawArrays/ElementsIndirect`：若单次 Indirect 可用则循环（处理 stride=0 紧密排列），
//!   否则无法模拟，告警返回
//! - `IndirectCount` 系列：无法可靠模拟，告警返回

use crate::backend;
use crate::backend::dispatch::GlesDispatch;

/// 判断 dispatch 函数指针是否为共享的未实现 stub。
fn is_stub(dispatch: &GlesDispatch, ptr: *const ()) -> bool {
    ptr == dispatch.stub as *const ()
}

/// DrawArraysIndirectCommand 布局（GLES 3.1 spec，16 字节紧密排列）
#[repr(C)]
struct DrawArraysIndirectCommand {
    count: u32,
    instance_count: u32,
    first: u32,
    base_instance: u32,
}

/// DrawElementsIndirectCommand 布局（GLES 3.1 spec，20 字节紧密排列）
#[repr(C)]
struct DrawElementsIndirectCommand {
    count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: u32,
    base_instance: u32,
}

/// 计算 indirect command 的步长。
/// stride=0 表示紧密排列，使用 command 结构体的实际大小。
fn array_indirect_stride(stride: isize) -> isize {
    if stride == 0 {
        std::mem::size_of::<DrawArraysIndirectCommand>() as isize
    } else {
        stride
    }
}

fn element_indirect_stride(stride: isize) -> isize {
    if stride == 0 {
        std::mem::size_of::<DrawElementsIndirectCommand>() as isize
    } else {
        stride
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawArrays(
    mode: u32,
    first: *const i32,
    count: *const i32,
    drawcount: i32,
) {
    if drawcount <= 0 {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.multi_draw_arrays as *const ()) {
            // 降级：循环 glDrawArrays
            for i in 0..drawcount as isize {
                (dispatch.draw_arrays)(mode, *first.offset(i), *count.offset(i));
            }
        } else {
            (dispatch.multi_draw_arrays)(mode, first, count, drawcount);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawElements(
    mode: u32,
    count: *const i32,
    type_: u32,
    indices: *const *const std::ffi::c_void,
    drawcount: i32,
) {
    if drawcount <= 0 {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.multi_draw_elements as *const ()) {
            // 降级：循环 glDrawElements
            for i in 0..drawcount as isize {
                (dispatch.draw_elements)(mode, *count.offset(i), type_, *indices.offset(i));
            }
        } else {
            (dispatch.multi_draw_elements)(mode, count, type_, indices, drawcount);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawElementsBaseVertex(
    mode: u32,
    count: *const i32,
    type_: u32,
    indices: *const *const std::ffi::c_void,
    drawcount: i32,
    basevertex: *const i32,
) {
    if drawcount <= 0 {
        return;
    }
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.multi_draw_elements_base_vertex
            && !is_stub(
                dispatch,
                dispatch.multi_draw_elements_base_vertex as *const (),
            );
        if !supported {
            // 优先尝试 glDrawElementsBaseVertex（保留 basevertex 语义）
            let base_vertex_ok = caps.draw_elements_base_vertex
                && !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
            if !base_vertex_ok {
                // 驱动完全不支持 basevertex：降级为普通 glDrawElements，丢弃 basevertex。
                // 注意：这会导致索引偏移错误，仅作 best-effort，避免崩溃。
                log::warn!(
                    "[FluorateGL] glMultiDrawElementsBaseVertex: GLES 不支持 basevertex，已降级为 glDrawElements 循环（索引偏移丢失）"
                );
                for i in 0..drawcount as isize {
                    (dispatch.draw_elements)(mode, *count.offset(i), type_, *indices.offset(i));
                }
            } else {
                for i in 0..drawcount as isize {
                    (dispatch.draw_elements_base_vertex)(
                        mode,
                        *count.offset(i),
                        type_,
                        *indices.offset(i),
                        *basevertex.offset(i),
                    );
                }
            }
        } else {
            (dispatch.multi_draw_elements_base_vertex)(
                mode, count, type_, indices, drawcount, basevertex,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawArraysIndirect(
    mode: u32,
    indirect: *const std::ffi::c_void,
    drawcount: i32,
    stride: isize,
) {
    if drawcount <= 0 {
        return;
    }
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.multi_draw_indirect
            && !is_stub(dispatch, dispatch.multi_draw_arrays_indirect as *const ());
        if !supported {
            // 降级：glDrawArraysIndirect 是 GLES 3.1 core（项目前提），直接循环调用
            let step = array_indirect_stride(stride);
            for i in 0..drawcount as isize {
                let cmd_ptr = (indirect as *const u8).offset(i * step) as *const std::ffi::c_void;
                (dispatch.draw_arrays_indirect)(mode, cmd_ptr);
            }
        } else {
            (dispatch.multi_draw_arrays_indirect)(mode, indirect, drawcount, stride);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawElementsIndirect(
    mode: u32,
    type_: u32,
    indirect: *const std::ffi::c_void,
    drawcount: i32,
    stride: isize,
) {
    if drawcount <= 0 {
        return;
    }
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.multi_draw_indirect
            && !is_stub(dispatch, dispatch.multi_draw_elements_indirect as *const ());
        if !supported {
            // 降级：glDrawElementsIndirect 是 GLES 3.1 core（项目前提），直接循环调用
            let step = element_indirect_stride(stride);
            for i in 0..drawcount as isize {
                let cmd_ptr = (indirect as *const u8).offset(i * step) as *const std::ffi::c_void;
                (dispatch.draw_elements_indirect)(mode, type_, cmd_ptr);
            }
        } else {
            (dispatch.multi_draw_elements_indirect)(mode, type_, indirect, drawcount, stride);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawArraysIndirectCount(
    mode: u32,
    indirect: *const std::ffi::c_void,
    drawcount: isize,
    maxdrawcount: i32,
    stride: isize,
) {
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.indirect_count
            && !is_stub(
                dispatch,
                dispatch.multi_draw_arrays_indirect_count as *const (),
            );
        if !supported {
            // IndirectCount 需从 GPU buffer 读取实际 drawcount，无法在 CPU 侧可靠模拟。
            // 仅当 maxdrawcount<=0 时可安全跳过，否则告警。
            if maxdrawcount > 0 {
                log::warn!(
                    "[FluorateGL] glMultiDrawArraysIndirectCount: GLES 不支持 indirect count，无法模拟，已跳过"
                );
            }
            return;
        }
        (dispatch.multi_draw_arrays_indirect_count)(
            mode,
            indirect,
            drawcount,
            maxdrawcount,
            stride,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMultiDrawElementsIndirectCount(
    mode: u32,
    type_: u32,
    indirect: *const std::ffi::c_void,
    drawcount: isize,
    maxdrawcount: i32,
    stride: isize,
) {
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.indirect_count
            && !is_stub(
                dispatch,
                dispatch.multi_draw_elements_indirect_count as *const (),
            );
        if !supported {
            if maxdrawcount > 0 {
                log::warn!(
                    "[FluorateGL] glMultiDrawElementsIndirectCount: GLES 不支持 indirect count，无法模拟，已跳过"
                );
            }
            return;
        }
        (dispatch.multi_draw_elements_indirect_count)(
            mode,
            type_,
            indirect,
            drawcount,
            maxdrawcount,
            stride,
        );
    });
}
