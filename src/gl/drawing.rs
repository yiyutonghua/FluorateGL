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
//! - Indirect 系列：GLES 3.1 core（项目前提），直接转发
//!
//! 持久映射 buffer 同步：每个 draw call 前调用 [`sync_persistent_buffer_if_needed`]
//! 同步 GL_ARRAY_BUFFER / GL_ELEMENT_ARRAY_BUFFER / GL_DRAW_INDIRECT_BUFFER 的脏区域。
//! 非持久映射 buffer 查询快速短路（两次 FxHashMap 查询），开销可忽略。

use crate::backend;
use crate::backend::dispatch::GlesDispatch;
use crate::gl::buffer::sync_persistent_buffer_if_needed;
use std::sync::atomic::{AtomicBool, Ordering};

/// GL_ARRAY_BUFFER target
const GL_ARRAY_BUFFER: u32 = 0x8892;
/// GL_ELEMENT_ARRAY_BUFFER target
const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
/// GL_DRAW_INDIRECT_BUFFER target
const GL_DRAW_INDIRECT_BUFFER: u32 = 0x8F3F;

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

/// 按 basevertex 偏移 indices 指针（BaseVertex 降级分支用）。
///
/// GL 3.3 core 语义：实际索引 = 索引值 + basevertex，等价于将 indices 指针按索引
/// 元素大小前移 basevertex 个元素。负 basevertex 时 offset 为负仍合法——宿主保证
/// index + basevertex ≥ 0，偏移后的指针仍落在同一 buffer 内。type_size 按 GL 索引
/// 类型推导：GL_UNSIGNED_BYTE(0x1401)=1、GL_UNSIGNED_SHORT(0x1403)=2、
/// GL_UNSIGNED_INT(0x1405)=4。
fn offset_indices(
    indices: *const std::ffi::c_void,
    basevertex: i32,
    type_: u32,
) -> *const std::ffi::c_void {
    if basevertex == 0 {
        return indices;
    }
    let type_size = match type_ {
        // GL_UNSIGNED_BYTE
        0x1401 => 1,
        // GL_UNSIGNED_SHORT
        0x1403 => 2,
        // GL_UNSIGNED_INT
        0x1405 => 4,
        _ => {
            log::error!(
                "[FluorateGL] offset_indices: 未知索引类型 0x{:04X}，无法计算 basevertex 偏移，按原指针降级",
                type_
            );
            return indices;
        }
    };
    unsafe {
        (indices as *const u8).offset(basevertex as isize * type_size) as *const std::ffi::c_void
    }
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
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
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
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
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
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
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
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 优先用扩展能力判断，is_stub 兜底（驱动声明扩展但未导出符号的边界情况）
        let supported = caps.draw_elements_base_vertex
            && !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
        if !supported {
            // 降级为普通 glDrawElements：用 basevertex 偏移 indices 指针补偿索引错位
            // （原实现丢弃 basevertex 导致顶点错位，仅 best-effort 避免崩溃）。
            warn_base_vertex_unsupported("glDrawElementsBaseVertex");
            let offset_indices_ptr = offset_indices(indices, basevertex, type_);
            (dispatch.draw_elements)(mode, count, type_, offset_indices_ptr);
        } else {
            (dispatch.draw_elements_base_vertex)(mode, count, type_, indices, basevertex);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysIndirect(mode: u32, indirect: *const std::ffi::c_void) {
    // 同步 indirect buffer 持久映射脏区域（若 indirect buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_DRAW_INDIRECT_BUFFER);
    log::debug!("[FluorateGL] glDrawArraysIndirect(mode=0x{:04X})", mode);
    // GLES 3.1 core 特性，项目前提，直接转发
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_arrays_indirect)(mode, indirect);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsIndirect(mode: u32, type_: u32, indirect: *const std::ffi::c_void) {
    // 同步 indirect buffer 持久映射脏区域（若 indirect buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_DRAW_INDIRECT_BUFFER);
    // M6: 索引 buffer 若为持久映射，数据过期会导致索引错乱，与 indirect buffer 一并同步
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    log::debug!(
        "[FluorateGL] glDrawElementsIndirect(mode=0x{:04X}, type=0x{:04X})",
        mode,
        type_
    );
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
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
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
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
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
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    let caps = backend::capabilities();
    backend::with_gles_dispatch(|dispatch| unsafe {
        let supported = caps.draw_elements_base_vertex
            && !is_stub(
                dispatch,
                dispatch.draw_elements_instanced_base_vertex as *const (),
            );
        if !supported {
            // 降级为 glDrawElementsInstanced：用 basevertex 偏移 indices 指针补偿索引错位
            warn_base_vertex_unsupported("glDrawElementsInstancedBaseVertex");
            let offset_indices_ptr = offset_indices(indices, basevertex, type_);
            (dispatch.draw_elements_instanced)(
                mode,
                count,
                type_,
                offset_indices_ptr,
                instancecount,
            );
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
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
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
            // 同时丢失 basevertex 和 baseinstance，触发两类首次告警；
            // basevertex 用 indices 指针偏移补偿，baseinstance 无法补偿
            warn_base_vertex_unsupported("glDrawElementsInstancedBaseVertexBaseInstance");
            warn_base_instance_unsupported("glDrawElementsInstancedBaseVertexBaseInstance");
            let offset_indices_ptr = offset_indices(indices, basevertex, type_);
            (dispatch.draw_elements_instanced)(
                mode,
                count,
                type_,
                offset_indices_ptr,
                instancecount,
            );
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
