//! Draw call 分发与降级
//!
//! 本模块处理非 Multi 的 draw call。Multi-draw 系列见 [`super::multi_draw`]。
//!
//! 决策依据（C1 修订）：**以 dispatch 函数指针存在性（`is_stub`）为主导**决定
//! 原生转发或模拟降级。caps 曾参与判定，但真机能力检测失败（version=0，
//! ANGLE glGetString 返回 null）时 caps 全 false，符号已加载的 3.2 函数被
//! 短路强制降级（BaseVertex 语义丢失）。符号在 = 驱动提供该函数，透传安全；
//! 符号缺失 = 走模拟降级（caps 仍用于 FAKE_EXTENSIONS 剔除等诊断场景）。
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

/// BaseVertex 精确降级：逐索引加法（P1 修复，对齐 MobileGlues drawing.cpp:160-247）。
///
/// 旧实现 offset_indices 用指针偏移（indices + basevertex×type_size），
/// 其等价性仅对顺序索引成立：`indices[i]+bv == indices[i+bv]` 需索引值
/// 连续递增。乱序/重复索引的 EBO 下两者不等价（如 EBO={0,5,2} + bv=3，
/// 正确=顶点{3,8,5}，指针偏移却读 EBO[3..]）→ 画错。
///
/// 本实现精确模拟 GL 3.3 语义"实际索引 = 索引值 + basevertex"：
/// 1. 读索引：绑定 EBO 时 map 读（draw 前已 sync shadow，数据一致）；
///    client 指针（无 EBO）直接拷贝
/// 2. 逐索引 +basevertex（u32 wrapping——索引越界是 app 错误，wrap 无害，
///    与 MobileGlues 直接加法一致）
/// 3. 写入临时 EBO（STREAM_DRAW）重画，恢复原绑定
///
/// `instancecount = Some(n)` 时走 instanced 版本（同 MobileGlues 无此路径，
/// 我们为 glDrawElementsInstancedBaseVertex 家族复用）。
/// 任何失败路径 best-effort 回退原 draw（与旧行为一致的降级），不崩溃。
///
/// pub(crate)：multi_draw.rs 的 glMultiDrawElementsBaseVertex 第三级降级共用
/// （P1：MultiDraw 逐 draw 精确模拟与单 draw 版行为对齐）。
pub(crate) fn draw_elements_basevertex_exact(
    dispatch: &GlesDispatch,
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    basevertex: i32,
    instancecount: Option<i32>,
) {
    if count <= 0 {
        return;
    }
    // 快速路径：basevertex=0 语义等同普通 draw（零转换开销）
    if basevertex == 0 {
        unsafe {
            match instancecount {
                Some(ic) => (dispatch.draw_elements_instanced)(mode, count, type_, indices, ic),
                None => (dispatch.draw_elements)(mode, count, type_, indices),
            }
        }
        return;
    }

    const GL_MAP_READ_BIT: u32 = 0x0001;
    const GL_STREAM_DRAW: u32 = 0x88E0;

    let index_size = match type_ {
        // GL_UNSIGNED_BYTE
        0x1401 => 1usize,
        // GL_UNSIGNED_SHORT
        0x1403 => 2,
        // GL_UNSIGNED_INT
        0x1405 => 4,
        _ => {
            log::error!(
                "[FluorateGL] draw_elements_basevertex_exact: 未知索引类型 0x{:04X}，无法精确模拟，按原 draw 降级",
                type_
            );
            unsafe {
                match instancecount {
                    Some(ic) => (dispatch.draw_elements_instanced)(mode, count, type_, indices, ic),
                    None => (dispatch.draw_elements)(mode, count, type_, indices),
                }
            }
            return;
        }
    };
    let n_bytes = (count as usize).saturating_mul(index_size);
    if n_bytes == 0 {
        return;
    }

    // 读索引源：绑定 EBO → map GLES buffer（draw 前 sync 保证数据最新）；
    // 无 EBO → client 指针
    let ebo_gles = crate::state::with_state_ref(|s| {
        s.bound_buffers_by_target
            .get(&GL_ELEMENT_ARRAY_BUFFER)
            .copied()
            .and_then(|d| s.buffers.get_gles(d))
    })
    .unwrap_or(0);

    let mut temp: Vec<u8> = Vec::with_capacity(n_bytes);
    unsafe {
        if ebo_gles != 0 {
            let src = (dispatch.map_buffer_range)(
                GL_ELEMENT_ARRAY_BUFFER,
                indices as isize,
                n_bytes as isize,
                GL_MAP_READ_BIT,
            );
            if src.is_null() {
                log::warn!(
                    "[FluorateGL] draw_elements_basevertex_exact: EBO map 失败（offset=0x{:x}），按原 draw best-effort 降级",
                    indices as usize
                );
                match instancecount {
                    Some(ic) => (dispatch.draw_elements_instanced)(mode, count, type_, indices, ic),
                    None => (dispatch.draw_elements)(mode, count, type_, indices),
                }
                return;
            }
            std::ptr::copy_nonoverlapping(src as *const u8, temp.as_mut_ptr(), n_bytes);
            (dispatch.unmap_buffer)(GL_ELEMENT_ARRAY_BUFFER);
        } else {
            // client 指针（无 EBO 绑定）
            std::ptr::copy_nonoverlapping(indices as *const u8, temp.as_mut_ptr(), n_bytes);
        }
    }

    // 逐索引 +basevertex（u32 wrapping，负 basevertex 由宿主保证索引+bv ≥ 0）
    match type_ {
        0x1401 => {
            for v in temp.iter_mut() {
                *v = v.wrapping_add(basevertex as u8);
            }
        }
        0x1403 => {
            for chunk in temp.chunks_exact_mut(2) {
                let v = u16::from_le_bytes([chunk[0], chunk[1]]).wrapping_add(basevertex as u16);
                let b = v.to_le_bytes();
                chunk[0] = b[0];
                chunk[1] = b[1];
            }
        }
        0x1405 => {
            for chunk in temp.chunks_exact_mut(4) {
                let v = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    .wrapping_add(basevertex as u32);
                let b = v.to_le_bytes();
                chunk.copy_from_slice(&b);
            }
        }
        _ => unreachable!(), // 上面已校验
    }

    // 临时 EBO 重画（MobileGlues 同款：gen → bind → data → draw → delete → 恢复）
    unsafe {
        let mut tmp_buf: u32 = 0;
        (dispatch.gen_buffers)(1, &mut tmp_buf);
        (dispatch.bind_buffer)(GL_ELEMENT_ARRAY_BUFFER, tmp_buf);
        (dispatch.buffer_data)(
            GL_ELEMENT_ARRAY_BUFFER,
            n_bytes as isize,
            temp.as_ptr() as *const std::ffi::c_void,
            GL_STREAM_DRAW,
        );
        match instancecount {
            Some(ic) => {
                (dispatch.draw_elements_instanced)(mode, count, type_, std::ptr::null(), ic)
            }
            None => (dispatch.draw_elements)(mode, count, type_, std::ptr::null()),
        }
        (dispatch.delete_buffers)(1, &mut tmp_buf);
        (dispatch.bind_buffer)(GL_ELEMENT_ARRAY_BUFFER, ebo_gles);
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：supported 以 dispatch 符号存在性为主导（is_stub 兜底）。
        // caps 曾参与判定——真机能力检测失败（version=0）时符号已加载也被
        // 短路强制降级，导致 basevertex 语义丢失（Sodium 渲染错误风险）。
        let supported = !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
        if !supported {
            // 降级：P1 逐索引加法精确模拟（读索引 + basevertex → 临时 EBO 重画）。
            // 旧实现指针偏移仅对顺序索引等价，乱序 EBO 画错。
            warn_base_vertex_unsupported("glDrawElementsBaseVertex");
            draw_elements_basevertex_exact(dispatch, mode, count, type_, indices, basevertex, None);
        } else {
            (dispatch.draw_elements_base_vertex)(mode, count, type_, indices, basevertex);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawRangeElementsBaseVertex(
    mode: u32,
    start: u32,
    end: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    basevertex: i32,
) {
    // D1：GL 3.3.1 core 导出补齐（此前 dispatch 有字段/加载但无导出符号，
    // LWJGL 绑定 null 有崩溃风险）。三级降级（同 basevertex 家族模式）：
    // 透传（GLES 3.2 core 同名）→ glDrawElementsBaseVertex（start/end 是 hint）
    // → P1 逐索引加法精确模拟 + glDrawElements
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导
        let supported = !is_stub(
            dispatch,
            dispatch.draw_range_elements_base_vertex as *const (),
        );
        if !supported {
            // 二级：降级为 glDrawElementsBaseVertex（start/end 是 hint，跳过不影响
            // 正确性——同 glDrawRangeElements 降级策略）
            let base_vertex_ok =
                !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
            if !base_vertex_ok {
                // 三级：P1 逐索引加法精确模拟
                warn_base_vertex_unsupported("glDrawRangeElementsBaseVertex");
                draw_elements_basevertex_exact(
                    dispatch, mode, count, type_, indices, basevertex, None,
                );
            } else {
                (dispatch.draw_elements_base_vertex)(mode, count, type_, indices, basevertex);
            }
        } else {
            (dispatch.draw_range_elements_base_vertex)(
                mode, start, end, count, type_, indices, basevertex,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysIndirect(mode: u32, indirect: *const std::ffi::c_void) {
    // 同步 indirect buffer 持久映射脏区域（若 indirect buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_DRAW_INDIRECT_BUFFER);
    // D3：同步 GL_ARRAY_BUFFER 持久映射脏区域（顶点数据 buffer——M6 审查只补了
    // ELEMENT，ARRAY 在 indirect 路径被系统性遗漏；Sodium 场景顶点 buffer 为
    // 持久映射 shadow，未同步会导致 indirect draw 使用过期顶点数据）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
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
    // D3：同步 GL_ARRAY_BUFFER 持久映射脏区域（顶点数据 buffer——M6 盲区）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（caps 误判 false 时不再强制降级）
        let supported = !is_stub(
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（caps 误判 false 时不再强制降级）
        let supported = !is_stub(
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（caps 误判 false 时不再强制降级）
        let supported = !is_stub(
            dispatch,
            dispatch.draw_elements_instanced_base_vertex as *const (),
        );
        if !supported {
            // 降级：P1 逐索引加法精确模拟（保留 instancecount）
            warn_base_vertex_unsupported("glDrawElementsInstancedBaseVertex");
            draw_elements_basevertex_exact(
                dispatch,
                mode,
                count,
                type_,
                indices,
                basevertex,
                Some(instancecount),
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（caps 误判 false 时不再强制降级）。
        // 该函数需要 base_vertex + base_instance 两个特性——符号在即两个都可用。
        let supported = !is_stub(
            dispatch,
            dispatch.draw_elements_instanced_base_vertex_base_instance as *const (),
        );
        if !supported {
            // 同时丢失 basevertex 和 baseinstance，触发两类首次告警；
            // basevertex 用 P1 逐索引加法精确模拟，baseinstance 无法补偿
            warn_base_vertex_unsupported("glDrawElementsInstancedBaseVertexBaseInstance");
            warn_base_instance_unsupported("glDrawElementsInstancedBaseVertexBaseInstance");
            draw_elements_basevertex_exact(
                dispatch,
                mode,
                count,
                type_,
                indices,
                basevertex,
                Some(instancecount),
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

/// GL 3.3 core §4.1：glDrawTransformFeedback / glDrawTransformFeedbackInstanced。
///
/// D2：导出补齐（此前缺失——LWJGL 绑定 null 有崩溃风险）。GLES 3.1 无对应
/// 函数（transform feedback 捕获回读绘制在 GLES 中不存在），语义无法模拟，
/// 故 stub no-op + 首次调用告警。调用方通常先查询扩展/版本再使用，实际
/// 触发概率低；导出符号存在即可避免 LWJGL 层 null 崩溃。
static TF_DRAW_WARNED: AtomicBool = AtomicBool::new(false);

fn warn_tf_draw_unsupported(fname: &str) {
    if !TF_DRAW_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 无 transform feedback 回读绘制（glDrawTransformFeedback），已 no-op，后续调用将静默跳过",
            fname
        );
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawTransformFeedback(_mode: u32, _id: u32) {
    warn_tf_draw_unsupported("glDrawTransformFeedback");
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawTransformFeedbackInstanced(_mode: u32, _id: u32, _instancecount: i32) {
    warn_tf_draw_unsupported("glDrawTransformFeedbackInstanced");
}

/// GL_SHADER_STORAGE_BARRIER_BIT（glMemoryBarrier 补位用；注意不是 target 值 0x90F2）
const GL_SHADER_STORAGE_BARRIER_BIT: u32 = 0x00002000;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDispatchCompute(num_groups_x: u32, num_groups_y: u32, num_groups_z: u32) {
    // P2：compute 分发的运行时载体（GLES 3.1 core 透传）。
    // 依赖：shader 翻译管线已把 atomic_uint 改写为 SSBO；app 的
    // GL_ATOMIC_COUNTER_BUFFER 绑定在 glBindBufferBase/Range 时已转发到
    // GL_SHADER_STORAGE_BUFFER（见 buffer.rs），此处无需额外处理。
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.dispatch_compute)(num_groups_x, num_groups_y, num_groups_z);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMemoryBarrier(barriers: u32) {
    // P2：补 GL_SHADER_STORAGE_BARRIER_BIT——atomic→SSBO 模拟后跨 dispatch 的
    // 可见性依赖 SSBO barrier（对齐 MobileGlues drawing.cpp:149-158；
    // OR 操作无副作用）。
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.memory_barrier)(barriers | GL_SHADER_STORAGE_BARRIER_BIT);
    });
}
