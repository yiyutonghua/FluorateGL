//! Multi-draw 系列函数拦截层
//!
//! 桌面 GL 的 `glMultiDraw*` 系列在一次调用中提交多个 draw command，减少 CPU 往返。
//!
//! 决策依据（C1 修订）：**以 dispatch 函数指针存在性（`is_stub`）为主导**决定
//! 原生转发或循环模拟。caps 曾参与判定，但真机能力检测失败（version=0）时
//! caps 全 false，符号已加载的 GLES 3.2 函数被短路强制降级。符号在 = 驱动
//! 提供该函数，透传安全；符号缺失 = 循环模拟（caps 仅作诊断参考）。
//!
//! stub 降级策略：
//! - `glMultiDrawArrays/Elements`：循环调用对应的单次 draw
//! - `glMultiDrawElementsBaseVertex`：优先循环 `glDrawElementsBaseVertex`（保留 basevertex），
//!   否则循环 `glDrawElements`（C3：用 `offset_indices` 按 basevertex 补偿索引指针，
//!   与单 draw 版 drawing.rs 行为一致）
//! - `glMultiDrawArrays/ElementsIndirect`：若单次 Indirect 可用则循环（处理 stride=0 紧密排列），
//!   否则无法模拟，告警返回
//! - `IndirectCount` 系列：GLES 不支持 GL_PARAMETER_BUFFER，通过 CPU 端读取 count buffer
//!   （shadow memory 优先，glMapBufferRange 兜底）后循环单次 Indirect 模拟
//!
//! 错误语义（C5/C6，对齐 GL 3.3 / GL 4.6）：
//! - `drawcount < 0`（IndirectCount 为 `maxdrawcount < 0`）→ 注入 GL_INVALID_VALUE
//! - `drawcount == 0` → 无操作且无错误（GL 4.5+ 明确；GL 3.3 对 0 同样无操作）
//! - IndirectCount 的 count buffer 值为负 → 不执行任何 draw（C6 修正 min() 偏差）

use crate::backend;
use crate::backend::dispatch::GlesDispatch;
use crate::gl::buffer::{read_parameter_buffer_u32, sync_persistent_buffer_if_needed};
use crate::gl::drawing::draw_elements_basevertex_exact;
use crate::gl::exports::inject_gl_error;

/// GL_INVALID_VALUE（0x0501）：drawcount/maxdrawcount 为负时的规范错误
const GL_INVALID_VALUE: u32 = 0x0501;

/// GL_ARRAY_BUFFER target
const GL_ARRAY_BUFFER: u32 = 0x8892;
/// GL_ELEMENT_ARRAY_BUFFER target
const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
/// GL_DRAW_INDIRECT_BUFFER：indirect command buffer 的 target（GLES 3.1 合法）
const GL_DRAW_INDIRECT_BUFFER: u32 = 0x8F3F;

/// 判断 dispatch 函数指针是否为共享的未实现 stub。
fn is_stub(dispatch: &GlesDispatch, ptr: *const ()) -> bool {
    ptr == dispatch.stub as *const ()
}

/// drawcount 语义统一入口：
/// - 负值：注入 GL_INVALID_VALUE（GL 3.3：drawcount 为负生成 INVALID_VALUE；
///   GL 4.5 放宽为 no-op——取 3.3 严格语义，与本项目北极星一致）
/// - 零：无操作无错误
/// 返回 true 表示调用方应立即 return（不执行任何 draw）。
fn handle_drawcount(drawcount: i32) -> bool {
    if drawcount < 0 {
        inject_gl_error(GL_INVALID_VALUE);
        return true;
    }
    drawcount == 0
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

/// 计算 indirect command 的步长（C7：stride 为 GLsizei/i32，与 GL 签名一致）。
/// stride=0 表示紧密排列，使用 command 结构体的实际大小。
fn array_indirect_stride(stride: i32) -> isize {
    if stride == 0 {
        std::mem::size_of::<DrawArraysIndirectCommand>() as isize
    } else {
        stride as isize
    }
}

fn element_indirect_stride(stride: i32) -> isize {
    if stride == 0 {
        std::mem::size_of::<DrawElementsIndirectCommand>() as isize
    } else {
        stride as isize
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
    // C5：drawcount<0 → GL_INVALID_VALUE（GL 3.3）；==0 → no-op 无错误
    if handle_drawcount(drawcount) {
        return;
    }
    // 同步 GL_ARRAY_BUFFER 持久映射的脏区域（若顶点 buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（GLES 无 core 名，EXT 后缀加载成功才透传）
        let supported = !is_stub(dispatch, dispatch.multi_draw_arrays as *const ());
        if !supported {
            // 降级：循环 glDrawArrays（GLES 2.0 core，恒可用）
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
    // C5：drawcount<0 → GL_INVALID_VALUE（GL 3.3）；==0 → no-op 无错误
    if handle_drawcount(drawcount) {
        return;
    }
    // 同步 GL_ARRAY_BUFFER / GL_ELEMENT_ARRAY_BUFFER 持久映射的脏区域
    // （若顶点/索引 buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（GLES 无 core 名，EXT 后缀加载成功才透传）
        let supported = !is_stub(dispatch, dispatch.multi_draw_elements as *const ());
        if !supported {
            // 降级：循环 glDrawElements（GLES 2.0 core，恒可用）
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
    // C5：drawcount<0 → GL_INVALID_VALUE（GL 3.3）；==0 → no-op 无错误
    if handle_drawcount(drawcount) {
        return;
    }
    // 同步 GL_ARRAY_BUFFER / GL_ELEMENT_ARRAY_BUFFER 持久映射的脏区域
    // （若顶点/索引 buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（GLES 3.2 core / EXT 后缀）
        let supported = !is_stub(
            dispatch,
            dispatch.multi_draw_elements_base_vertex as *const (),
        );
        if !supported {
            // 优先尝试 glDrawElementsBaseVertex（保留 basevertex 语义）
            // C1：同以符号存在性为主导
            let base_vertex_ok = !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
            if !base_vertex_ok {
                // 驱动完全不支持 basevertex：降级循环，
                // P1：每 draw 用逐索引加法精确模拟（读索引 + basevertex → 临时 EBO）。
                // 旧 C3 指针偏移仅对顺序索引等价，乱序 EBO 画错。
                log::warn!(
                    "[FluorateGL] glMultiDrawElementsBaseVertex: GLES 不支持 basevertex，已降级为逐索引加法循环（精确模拟）"
                );
                for i in 0..drawcount as isize {
                    draw_elements_basevertex_exact(
                        dispatch,
                        mode,
                        *count.offset(i),
                        type_,
                        *indices.offset(i),
                        *basevertex.offset(i),
                        None,
                    );
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
    stride: i32,
) {
    // C5：drawcount<0 → GL_INVALID_VALUE（GL 3.3）；==0 → no-op 无错误
    if handle_drawcount(drawcount) {
        return;
    }
    log::debug!(
        "[FluorateGL] glMultiDrawArraysIndirect(mode=0x{:04X}, drawcount={}, stride={})",
        mode,
        drawcount,
        stride
    );
    // 同步 GL_DRAW_INDIRECT_BUFFER 持久映射的脏区域（若 indirect buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_DRAW_INDIRECT_BUFFER);
    // D3：同步 GL_ARRAY_BUFFER 持久映射的脏区域（顶点数据 buffer 持久映射时
    // 未上传会导致 indirect draw 使用过期顶点数据——M6 只补了 ELEMENT，ARRAY 是盲区）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（GLES 3.2 core / GL_EXT_multi_draw_indirect）
        let supported = !is_stub(dispatch, dispatch.multi_draw_arrays_indirect as *const ());
        if !supported {
            // D5：GL 规范 stride 必须 ≥ 0，负值 → GL_INVALID_VALUE。
            // 放在降级分支内：透传路径由 GLES 驱动报错（不注入避免双重错误）
            if stride < 0 {
                inject_gl_error(GL_INVALID_VALUE);
                return;
            }
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
    stride: i32,
) {
    // C5：drawcount<0 → GL_INVALID_VALUE（GL 3.3）；==0 → no-op 无错误
    if handle_drawcount(drawcount) {
        return;
    }
    log::debug!(
        "[FluorateGL] glMultiDrawElementsIndirect(mode=0x{:04X}, type=0x{:04X}, drawcount={}, stride={})",
        mode,
        type_,
        drawcount,
        stride
    );
    // 同步 GL_DRAW_INDIRECT_BUFFER 持久映射的脏区域（若 indirect buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_DRAW_INDIRECT_BUFFER);
    // 同步 GL_ELEMENT_ARRAY_BUFFER 持久映射的脏区域（若索引 buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    // D3：同步 GL_ARRAY_BUFFER 持久映射的脏区域（顶点数据 buffer，M6 盲区）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（GLES 3.2 core / GL_EXT_multi_draw_indirect）
        let supported = !is_stub(dispatch, dispatch.multi_draw_elements_indirect as *const ());
        if !supported {
            // D5：GL 规范 stride 必须 ≥ 0，负值 → GL_INVALID_VALUE（降级分支内，
            // 透传路径由 GLES 驱动报错）
            if stride < 0 {
                inject_gl_error(GL_INVALID_VALUE);
                return;
            }
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
    stride: i32,
) {
    // C5：maxdrawcount<0 → GL_INVALID_VALUE（GL 4.6）；==0 → no-op 无错误
    if handle_drawcount(maxdrawcount) {
        return;
    }

    // 同步 indirect buffer 持久映射脏区域
    sync_persistent_buffer_if_needed(GL_DRAW_INDIRECT_BUFFER);

    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（GLES 无对应，恒 stub → 恒 CPU 模拟）
        let supported = !is_stub(
            dispatch,
            dispatch.multi_draw_arrays_indirect_count as *const (),
        );
        if supported {
            (dispatch.multi_draw_arrays_indirect_count)(
                mode,
                indirect,
                drawcount,
                maxdrawcount,
                stride,
            );
            return;
        }
        // D5：GL 4.6 规范 stride 必须 ≥ 0，负值 → GL_INVALID_VALUE（CPU 模拟路径）
        if stride < 0 {
            inject_gl_error(GL_INVALID_VALUE);
            return;
        }
        // 降级：CPU 端读取 count buffer 的实际 drawcount，循环 glDrawArraysIndirect
        let Some(raw) = read_parameter_buffer_u32(drawcount) else {
            // count buffer 未绑定或读取失败，按 maxdrawcount 兜底（best-effort）
            log::debug!(
                "[FluorateGL] glMultiDrawArraysIndirectCount: count buffer 读取失败，使用 maxdrawcount={} 兜底",
                maxdrawcount
            );
            let step = array_indirect_stride(stride);
            for i in 0..maxdrawcount as isize {
                let cmd_ptr = (indirect as *const u8).offset(i * step) as *const std::ffi::c_void;
                (dispatch.draw_arrays_indirect)(mode, cmd_ptr);
            }
            return;
        };
        // C6：count buffer 值按 GLsizei 解释，负值 = 不执行任何 draw
        // （GL 4.6：值为负时不画；原 min() 把负值当大数导致画满 maxdrawcount）
        let actual = if (raw as i32) < 0 {
            0
        } else {
            (raw as i32).min(maxdrawcount)
        };
        let step = array_indirect_stride(stride);
        for i in 0..actual as isize {
            let cmd_ptr = (indirect as *const u8).offset(i * step) as *const std::ffi::c_void;
            (dispatch.draw_arrays_indirect)(mode, cmd_ptr);
        }
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
    stride: i32,
) {
    // C5：maxdrawcount<0 → GL_INVALID_VALUE（GL 4.6）；==0 → no-op 无错误
    if handle_drawcount(maxdrawcount) {
        return;
    }

    // 同步 indirect buffer 持久映射脏区域
    sync_persistent_buffer_if_needed(GL_DRAW_INDIRECT_BUFFER);
    // D4：同步 GL_ELEMENT_ARRAY_BUFFER 持久映射的脏区域（索引 buffer——与
    // glDrawElementsIndirect/glMultiDrawElementsIndirect 的 M6 补丁对齐，
    // 该函数此前遗漏）
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);

    backend::with_gles_dispatch(|dispatch| unsafe {
        // C1：以符号存在性为主导（GLES 无对应，恒 stub → 恒 CPU 模拟）
        let supported = !is_stub(
            dispatch,
            dispatch.multi_draw_elements_indirect_count as *const (),
        );
        if supported {
            (dispatch.multi_draw_elements_indirect_count)(
                mode,
                type_,
                indirect,
                drawcount,
                maxdrawcount,
                stride,
            );
            return;
        }
        // D5：GL 4.6 规范 stride 必须 ≥ 0，负值 → GL_INVALID_VALUE（CPU 模拟路径）
        if stride < 0 {
            inject_gl_error(GL_INVALID_VALUE);
            return;
        }
        // 降级：CPU 端读取 count buffer 的实际 drawcount，循环 glDrawElementsIndirect
        let Some(raw) = read_parameter_buffer_u32(drawcount) else {
            log::debug!(
                "[FluorateGL] glMultiDrawElementsIndirectCount: count buffer 读取失败，使用 maxdrawcount={} 兜底",
                maxdrawcount
            );
            let step = element_indirect_stride(stride);
            for i in 0..maxdrawcount as isize {
                let cmd_ptr = (indirect as *const u8).offset(i * step) as *const std::ffi::c_void;
                (dispatch.draw_elements_indirect)(mode, type_, cmd_ptr);
            }
            return;
        };
        // C6：count buffer 值按 GLsizei 解释，负值 = 不执行任何 draw
        let actual = if (raw as i32) < 0 {
            0
        } else {
            (raw as i32).min(maxdrawcount)
        };
        let step = element_indirect_stride(stride);
        for i in 0..actual as isize {
            let cmd_ptr = (indirect as *const u8).offset(i * step) as *const std::ffi::c_void;
            (dispatch.draw_elements_indirect)(mode, type_, cmd_ptr);
        }
    });
}
