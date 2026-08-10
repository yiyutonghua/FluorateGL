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
//!   否则循环 `glDrawElements`（P1：逐索引加法精确模拟，与单 draw 版 drawing.rs 行为一致）
//! - `glMultiDrawArrays/ElementsIndirect`：若单次 Indirect 可用则循环（处理 stride=0 紧密排列），
//!   否则无法模拟，告警返回
//! - `IndirectCount` 系列：GLES 不支持 GL_PARAMETER_BUFFER，通过 CPU 端读取 count buffer
//!   （shadow memory 优先，glMapBufferRange 兜底——域 1 已实现 MG 语义）后循环单次
//!   Indirect 模拟。D4-2：GPU compute compaction（MG multidraw.cpp:1994-2108）已完整
//!   移植为主路径（count 仍由 CPU shadow 读——零 stall，上传 4 字节后 compaction
//!   shader 处理命令），任何失败自动回退 CPU 循环路径（fail-open）。
//!
//! Primitive restart（D4）：对齐 MG multidraw.cpp 的 restart 处理——
//! - 自定义 restart index（`restart_needs_rewrite`）：batch 强制走逐 draw 索引流重写
//!   （MG mg_multidraw_restart_takeover 同款：只有重写后端处理哨兵，其他后端
//!   把 app 索引原样交给驱动）
//! - 固定哨兵：驱动 FIXED_INDEX 已由 exports.rs 的 glEnable 翻译生效，零开销直通
//! - Indirect 系列：命令流在 GPU buffer 中无法重写，固定哨兵生效 + 自定义 index
//!   告警一次（MG MD_WARN_ONCE 同款）
//!
//! 错误语义（C5/C6，对齐 GL 3.3 / GL 4.6）：
//! - `drawcount < 0`（IndirectCount 为 `maxdrawcount < 0`）→ 注入 GL_INVALID_VALUE
//! - `drawcount == 0` → 无操作且无错误（GL 4.5+ 明确；GL 3.3 对 0 同样无操作）
//! - IndirectCount 的 count buffer 值为负 → 不执行任何 draw（C6 修正 min() 偏差）
//!
//! 持久映射 buffer 同步：`sync_persistent_buffer_if_needed` 已被域 1 删除，
//! 本模块不再有同步调用点（D4 清理，共 14 处）。

use crate::backend;
use crate::backend::dispatch::GlesDispatch;
use crate::gl::buffer::read_parameter_buffer_u32;
use crate::gl::drawing::{
    draw_elements_basevertex_exact, draw_elements_restart_rewrite, prepare_for_draw,
    restart_needs_rewrite,
};
use crate::gl::exports::inject_gl_error;
use std::sync::atomic::{AtomicBool, Ordering};

/// GL_INVALID_VALUE（0x0501）：drawcount/maxdrawcount 为负时的规范错误
const GL_INVALID_VALUE: u32 = 0x0501;

/// 首次告警：indirect draw 上自定义 restart index 无法模拟（MG MD_WARN_ONCE 同款）。
static RESTART_INDIRECT_WARNED: AtomicBool = AtomicBool::new(false);

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
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // C1：以符号存在性为主导（GLES 无 core 名，EXT 后缀加载成功才透传）
        let supported = !is_stub(dispatch, dispatch.multi_draw_arrays as *const ());
        if !supported {
            // 降级：循环 glDrawArrays（GLES 2.0 core，恒可用；MG
            // mg_glMultiDrawArrays_unroll 同款——GL 4.6 §10.5 定义即此循环）
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先（MG mg_multidraw_restart_takeover：自定义
        // restart index 的 batch 强制走逐 draw 重写——其他后端把 app 索引
        // 原样交给驱动，哨兵不生效）
        let needs_rewrite = restart_needs_rewrite(dispatch, type_);
        // C1：以符号存在性为主导（GLES 无 core 名，EXT 后缀加载成功才透传）
        let supported = !is_stub(dispatch, dispatch.multi_draw_elements as *const ());
        if supported && !needs_rewrite {
            (dispatch.multi_draw_elements)(mode, count, type_, indices, drawcount);
            return;
        }
        // 降级：循环 glDrawElements（GLES 2.0 core，恒可用）
        for i in 0..drawcount as isize {
            let c = *count.offset(i);
            let idx = *indices.offset(i);
            if needs_rewrite && draw_elements_restart_rewrite(dispatch, mode, c, type_, idx, 0, -1)
            {
                continue;
            }
            (dispatch.draw_elements)(mode, c, type_, idx);
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先（MG mg_multidraw_restart_takeover 同款：
        // 需要重写时整个 batch 走逐 draw 重写循环，重写同时应用 basevertex）
        let needs_rewrite = restart_needs_rewrite(dispatch, type_);
        // C1：以符号存在性为主导（GLES 3.2 core / EXT 后缀）
        let supported = !is_stub(
            dispatch,
            dispatch.multi_draw_elements_base_vertex as *const (),
        );
        if supported && !needs_rewrite {
            (dispatch.multi_draw_elements_base_vertex)(
                mode, count, type_, indices, drawcount, basevertex,
            );
            return;
        }
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
                let c = *count.offset(i);
                let idx = *indices.offset(i);
                let bv = *basevertex.offset(i);
                if needs_rewrite
                    && draw_elements_restart_rewrite(dispatch, mode, c, type_, idx, bv, -1)
                {
                    continue;
                }
                draw_elements_basevertex_exact(dispatch, mode, c, type_, idx, bv, None);
            }
        } else {
            for i in 0..drawcount as isize {
                let c = *count.offset(i);
                let idx = *indices.offset(i);
                let bv = *basevertex.offset(i);
                if needs_rewrite
                    && draw_elements_restart_rewrite(dispatch, mode, c, type_, idx, bv, -1)
                {
                    continue;
                }
                (dispatch.draw_elements_base_vertex)(mode, c, type_, idx, bv);
            }
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
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
            // （MG multidraw.cpp glMultiDrawArraysIndirect 同款循环模拟：
            // stride=0 表示紧密排列）
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart——命令流在 GPU buffer 中无法重写（MG 同款：固定哨兵
        // 场景由驱动 FIXED_INDEX 生效；自定义 index 无法模拟，告警一次）
        if restart_needs_rewrite(dispatch, type_) {
            if !RESTART_INDIRECT_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "[FluorateGL] glMultiDrawElementsIndirect: GL_PRIMITIVE_RESTART 自定义索引无法在 indirect draw 模拟，restart 将被忽略"
                );
            }
        }
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

    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
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
        // D4-2：GPU compaction 优先（MG multidraw.cpp mg_indirect_count 移植）；
        // 任何失败自动回退下方 CPU 路径（fail-open）
        if mg_indirect_count(
            dispatch,
            mode,
            0,
            false,
            indirect,
            drawcount,
            maxdrawcount,
            stride,
        ) {
            return;
        }
        // 降级：CPU 端读取 count buffer 的实际 drawcount，循环 glDrawArraysIndirect
        // （域 1 已实现 read_parameter_buffer_u32 的 MG 语义：shadow 优先 +
        // glMapBufferRange 兜底；drawcount 为字节偏移，直传）
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

    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart——同 glMultiDrawElementsIndirect（命令流不可重写，
        // 固定哨兵由驱动生效，自定义 index 告警一次）
        if restart_needs_rewrite(dispatch, type_) {
            if !RESTART_INDIRECT_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "[FluorateGL] glMultiDrawElementsIndirectCount: GL_PRIMITIVE_RESTART 自定义索引无法在 indirect draw 模拟，restart 将被忽略"
                );
            }
        }
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
        // D4-2：GPU compaction 优先；失败自动回退下方 CPU 路径（fail-open）
        if mg_indirect_count(
            dispatch,
            mode,
            type_,
            true,
            indirect,
            drawcount,
            maxdrawcount,
            stride,
        ) {
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

// ---------------------------------------------------------------------------
// IndirectCount GPU compaction（D4-2：MG multidraw.cpp:1994-2108 完整移植）
//
// MG 用 compute shader 把 maxdrawcount 条命令压入 scratch buffer（越界命令
// instanceCount 置 0），再批量 glMultiDraw*IndirectEXT，避免 CPU 回读 count
// 的 pipeline stall。
//
// 我们的架构适配：
// - 参数 buffer：我们的 GL_PARAMETER_BUFFER 是 shadow-only（buffer.rs 域 1
//   设计，无 GLES buffer）→ compaction 前用 read_parameter_buffer_u32
//   （shadow 读零 stall）取 count，上传到 scratch 参数 buffer（4 字节，
//   shader 的 uCountWord 恒 0）
// - 批量 draw：dispatch 无 EXT 后缀符号（未加载 glMultiDraw*IndirectEXT），
//   用循环单次 indirect draw（MG 的 fallback 分支同款——越界命令
//   instanceCount=0，逐条画结果一致）
// - 上下文隔离：缓存放 thread_local（与 State 同款，线程 = 上下文，等效
//   MG 的 ctx_id 失效检查）
// - 编译旁路：内置 GLSL 310 es 经 dispatch 底层函数直接编译（绕过导出层
//   与 shader 翻译管线——MG 同款 GLES.glCompileShader 直连驱动）
// - 失败兜底（fail-open）：任何失败返回 false → 调用方走 CPU
//   read_parameter_buffer_u32 循环路径（语义与第一轮完全一致）
// ---------------------------------------------------------------------------

/// 内置 compaction compute shader（MG multidraw.cpp multidraw_count_shader 原样）。
const MULTIDRAW_COUNT_SHADER: &str = r#"#version 310 es

layout(local_size_x = 64) in;

layout(location = 0) uniform uint uMaxDrawCount;
layout(location = 1) uniform uint uSrcWords;   // stride in words between source commands
layout(location = 2) uniform uint uSrcOffset;  // byte offset of the first command, in words
layout(location = 3) uniform uint uCountWord;  // byte offset of the count, in words
layout(location = 4) uniform uint uDstWords;   // words per command (4 arrays, 5 elements)

layout(std430, binding = 0) readonly buffer Src { uint src[]; };
layout(std430, binding = 1) readonly buffer Param { uint param[]; };
layout(std430, binding = 2) writeonly buffer Dst { uint dst[]; };

void main() {
    uint i = gl_GlobalInvocationID.x;
    if (i >= uMaxDrawCount) {
        return;
    }

    // Only decides whether this command is kept. The read bounds come from the
    // early return above and from the host-side check that maxdrawcount commands
    // fit inside the source buffer, so a corrupt count cannot widen them.
    uint realCount = param[uCountWord];

    uint sbase = uSrcOffset + i * uSrcWords;
    uint dbase = i * uDstWords;
    for (uint w = 0u; w < uDstWords; ++w) {
        dst[dbase + w] = src[sbase + w];
    }
    if (i >= realCount) {
        dst[dbase + 1u] = 0u; // instanceCount, word 1 of both command layouts
    }
}
"#;

/// compaction 缓存的 GL 对象（驱动侧 GLES id；thread_local 持有）。
#[derive(Clone, Copy)]
struct CountComputeCache {
    program: u32,
    scratch_param: u32,
    scratch_dst: u32,
    loc_max: i32,
    loc_srcwords: i32,
    loc_srcoff: i32,
    loc_cntoff: i32,
    loc_dstwords: i32,
}

thread_local! {
    static COUNT_COMPUTE: std::cell::RefCell<Option<CountComputeCache>> =
        std::cell::RefCell::new(None);
    /// 编译失败 latch：本线程（本上下文）内不再重试编译（MG g_count_failed 同款）。
    static COUNT_COMPUTE_FAILED: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// 编译 compaction 程序并创建 scratch buffers（MG compile_compute_program +
/// mg_count_init）。失败返回 None（调用方走 CPU 兜底）。
fn count_compute_init(dispatch: &GlesDispatch) -> Option<CountComputeCache> {
    const GL_COMPUTE_SHADER: u32 = 0x91B9;
    const GL_COMPILE_STATUS: u32 = 0x8B81;
    const GL_LINK_STATUS: u32 = 0x8B82;

    let shader = unsafe { (dispatch.create_shader)(GL_COMPUTE_SHADER) };
    let program = unsafe { (dispatch.create_program)() };
    if shader == 0 || program == 0 {
        if shader != 0 {
            unsafe { (dispatch.delete_shader)(shader) }
        }
        if program != 0 {
            unsafe { (dispatch.delete_program)(program) }
        }
        log::warn!(
            "[FluorateGL] IndirectCount compaction: compute shader 不可用（create 失败），回退 CPU 路径"
        );
        return None;
    }
    let src = match std::ffi::CString::new(MULTIDRAW_COUNT_SHADER) {
        Ok(s) => s,
        Err(_) => {
            unsafe {
                (dispatch.delete_shader)(shader);
                (dispatch.delete_program)(program);
            }
            return None;
        }
    };
    let ptr = src.as_ptr();
    let len = src.as_bytes().len() as i32;
    unsafe {
        (dispatch.shader_source)(shader, 1, &ptr, &len);
        (dispatch.compile_shader)(shader);
    }
    let mut ok = 0i32;
    unsafe { (dispatch.get_shader_iv)(shader, GL_COMPILE_STATUS, &mut ok) }
    if ok == 0 {
        log_compile_failure(dispatch, shader, false);
        unsafe {
            (dispatch.delete_shader)(shader);
            (dispatch.delete_program)(program);
        }
        return None;
    }
    unsafe {
        (dispatch.attach_shader)(program, shader);
        (dispatch.delete_shader)(shader);
        (dispatch.link_program)(program);
    }
    let mut ok = 0i32;
    unsafe { (dispatch.get_program_iv)(program, GL_LINK_STATUS, &mut ok) }
    if ok == 0 {
        log_compile_failure(dispatch, program, true);
        unsafe { (dispatch.delete_program)(program) }
        return None;
    }

    // uniform 定位（MG：任何缺失 → 禁用 compaction）
    let name = |n: &'static [u8]| unsafe {
        (dispatch.get_uniform_location)(program, n.as_ptr() as *const std::ffi::c_char)
    };
    let loc_max = name(b"uMaxDrawCount\0");
    let loc_srcwords = name(b"uSrcWords\0");
    let loc_srcoff = name(b"uSrcOffset\0");
    let loc_cntoff = name(b"uCountWord\0");
    let loc_dstwords = name(b"uDstWords\0");
    if loc_max < 0 || loc_srcwords < 0 || loc_srcoff < 0 || loc_cntoff < 0 || loc_dstwords < 0 {
        log::warn!(
            "[FluorateGL] IndirectCount compaction: uniform 定位缺失（max={} srcwords={} srcoff={} cntoff={} dstwords={}），禁用",
            loc_max,
            loc_srcwords,
            loc_srcoff,
            loc_cntoff,
            loc_dstwords
        );
        unsafe { (dispatch.delete_program)(program) }
        return None;
    }

    let mut scratch_param = 0u32;
    let mut scratch_dst = 0u32;
    unsafe {
        (dispatch.gen_buffers)(1, &mut scratch_param);
        (dispatch.gen_buffers)(1, &mut scratch_dst);
    }
    Some(CountComputeCache {
        program,
        scratch_param,
        scratch_dst,
        loc_max,
        loc_srcwords,
        loc_srcoff,
        loc_cntoff,
        loc_dstwords,
    })
}

/// 编译/link 失败日志（MG compile_compute_program 的 LOG_W_FORCE 同款）。
fn log_compile_failure(dispatch: &GlesDispatch, obj: u32, is_program: bool) {
    const GL_INFO_LOG_LENGTH: u32 = 0x8B84;
    let mut len = 0i32;
    let mut buf: Vec<u8> = Vec::new();
    unsafe {
        if is_program {
            (dispatch.get_program_iv)(obj, GL_INFO_LOG_LENGTH, &mut len);
        } else {
            (dispatch.get_shader_iv)(obj, GL_INFO_LOG_LENGTH, &mut len);
        }
        if len > 0 {
            buf.resize(len as usize, 0u8);
            let p = buf.as_mut_ptr() as *mut std::ffi::c_char;
            if is_program {
                (dispatch.get_program_info_log)(obj, len, &mut len, p);
            } else {
                (dispatch.get_shader_info_log)(obj, len, &mut len, p);
            }
        }
    }
    let info = if buf.is_empty() {
        "(no info log)".to_string()
    } else {
        String::from_utf8_lossy(&buf).trim().to_string()
    };
    log::warn!(
        "[FluorateGL] IndirectCount compaction: {} {} 失败: {}，回退 CPU 路径",
        if is_program {
            "program link"
        } else {
            "shader compile"
        },
        obj,
        info
    );
}

/// MG multidraw.cpp mg_indirect_count 移植：GPU compaction 后逐条 indirect draw。
///
/// 返回 true = 已执行（含"count 为负无操作"）；false = 无法 compaction，
/// 调用方必须走 CPU 兜底路径（fail-open）。
#[allow(clippy::too_many_arguments)]
fn mg_indirect_count(
    dispatch: &GlesDispatch,
    mode: u32,
    type_: u32,
    is_elements: bool,
    indirect: *const std::ffi::c_void,
    drawcount: isize,
    maxdrawcount: i32,
    stride: i32,
) -> bool {
    const GL_BUFFER_SIZE: u32 = 0x8764;
    const GL_SHADER_STORAGE_BUFFER: u32 = 0x90D2;
    const GL_SHADER_STORAGE_BUFFER_BINDING: u32 = 0x90D3;
    const GL_SHADER_STORAGE_BUFFER_START: u32 = 0x90D4;
    const GL_SHADER_STORAGE_BUFFER_SIZE: u32 = 0x90D5;
    const GL_MAX_COMPUTE_WORK_GROUP_COUNT: u32 = 0x91BE;
    const GL_CURRENT_PROGRAM: u32 = 0x8B8D;
    const GL_DYNAMIC_DRAW: u32 = 0x88E8;
    const GL_SHADER_STORAGE_BARRIER_BIT: u32 = 0x00002000;
    const GL_COMMAND_BARRIER_BIT: u32 = 0x00000020;
    const GL_DRAW_INDIRECT_BUFFER: u32 = 0x8F3F;
    const GL_NO_ERROR: u32 = 0;

    if maxdrawcount <= 0 {
        return true; // 无可画，非错误（与 handle_drawcount 语义一致）
    }
    let cmd_bytes: isize = if is_elements { 20 } else { 16 };
    let src_stride: isize = if stride == 0 {
        cmd_bytes
    } else {
        stride as isize
    };
    let src_off = indirect as usize;

    // 对齐校验（MG：stride/偏移必须为非负 4 的倍数——shader 以 uint 数组索引；
    // 不满足 → CPU 兜底。drawcount<0 也在此拦截 → CPU 路径（其语义为不执行））
    if stride < 0
        || (src_stride % 4) != 0
        || (src_off % 4) != 0
        || (drawcount % 4) != 0
        || drawcount < 0
    {
        log::debug!(
            "[FluorateGL] IndirectCount compaction: stride/偏移非 4 对齐或非法，走 CPU 路径"
        );
        return false;
    }
    if src_stride < cmd_bytes {
        log::debug!(
            "[FluorateGL] IndirectCount compaction: stride {} 小于单命令 {} 字节，走 CPU 路径",
            src_stride,
            cmd_bytes
        );
        return false;
    }

    // 读 count（shadow 优先零 stall；C6：负值 = 不执行任何 draw——shader 按
    // uint 读无法表达负值语义，CPU 侧预判）
    let Some(raw) = read_parameter_buffer_u32(drawcount) else {
        log::debug!(
            "[FluorateGL] IndirectCount compaction: count buffer 读取失败，走 CPU 路径（maxdrawcount 兜底）"
        );
        return false;
    };
    if (raw as i32) < 0 {
        return true;
    }

    // 缓存 / 编译（失败 latch 后不再重试）
    if COUNT_COMPUTE_FAILED.with(|f| f.get()) {
        return false;
    }
    let cc: Option<CountComputeCache> = COUNT_COMPUTE.with(|c| {
        let mut opt = c.borrow_mut();
        if opt.is_none() {
            match count_compute_init(dispatch) {
                Some(cache) => *opt = Some(cache),
                None => {
                    COUNT_COMPUTE_FAILED.with(|f| f.set(true));
                    return None;
                }
            }
        }
        opt.as_ref().copied()
    });
    let Some(cc) = cc else {
        return false;
    };

    // 保存状态（MG：先保存再动任何绑定）
    let mut prev_ssbo = 0i32;
    let mut prev_program = 0i32;
    unsafe {
        (dispatch.get_integerv)(GL_SHADER_STORAGE_BUFFER_BINDING, &mut prev_ssbo);
        (dispatch.get_integerv)(GL_CURRENT_PROGRAM, &mut prev_program);
    }
    let prev_indirect = crate::state::with_state_ref(|s| {
        s.bound_buffers_by_target
            .get(&GL_DRAW_INDIRECT_BUFFER)
            .copied()
            .and_then(|d| s.buffers.get_gles(d))
    })
    .unwrap_or(0);
    if prev_indirect == 0 {
        log::warn!(
            "[FluorateGL] IndirectCount compaction: 无 GL_DRAW_INDIRECT_BUFFER 绑定，走 CPU 路径"
        );
        return false;
    }

    // SSBO indexed 绑定保存（0..3；Range 恢复依赖 get_integer64i_v——GLES
    // 3.2/EXT 才有，缺失时仅恢复 base 绑定，Range 语义静默变宽）
    let have_64 = !is_stub(dispatch, dispatch.get_integer64i_v as *const ());
    let mut prev_base = [0i32; 3];
    let mut prev_start = [0i64; 3];
    let mut prev_size = [0i64; 3];
    unsafe {
        for i in 0..3 {
            (dispatch.get_integeri_v)(
                GL_SHADER_STORAGE_BUFFER_BINDING,
                i as u32,
                &mut prev_base[i],
            );
        }
        if have_64 {
            for i in 0..3 {
                (dispatch.get_integer64i_v)(
                    GL_SHADER_STORAGE_BUFFER_START,
                    i as u32,
                    &mut prev_start[i],
                );
                (dispatch.get_integer64i_v)(
                    GL_SHADER_STORAGE_BUFFER_SIZE,
                    i as u32,
                    &mut prev_size[i],
                );
            }
        }
    }
    // 恢复闭包（MG restore_ssbo 同款：Range 绑定必须按 Range 恢复，否则静默变宽）
    let restore_ssbo = |d: &GlesDispatch| unsafe {
        for i in 0..3 {
            if have_64 && prev_base[i] != 0 && prev_size[i] > 0 {
                (d.bind_buffer_range)(
                    GL_SHADER_STORAGE_BUFFER,
                    i as u32,
                    prev_base[i] as u32,
                    prev_start[i] as isize,
                    prev_size[i] as isize,
                );
            } else {
                (d.bind_buffer_base)(GL_SHADER_STORAGE_BUFFER, i as u32, prev_base[i] as u32);
            }
        }
        (d.bind_buffer)(GL_SHADER_STORAGE_BUFFER, prev_ssbo as u32);
    };

    // 源命令范围校验（MG：shader 越界 SSBO 访问是 undefined 而非错误）
    let src_span =
        src_off as u64 + (maxdrawcount as u64 - 1) * src_stride as u64 + cmd_bytes as u64;
    let mut src_size = 0i32;
    unsafe {
        (dispatch.get_buffer_parameter_iv)(GL_DRAW_INDIRECT_BUFFER, GL_BUFFER_SIZE, &mut src_size)
    }
    if src_size < 0 || src_span > src_size as u64 {
        log::warn!(
            "[FluorateGL] IndirectCount compaction: 命令范围超出 indirect buffer（{} > {}），走 CPU 路径",
            src_span,
            src_size
        );
        return false;
    }

    // count 上传 scratch_param（4 字节；我们的 GL_PARAMETER_BUFFER 是 shadow-only）
    let mut param_size = 0i32;
    unsafe {
        (dispatch.bind_buffer)(GL_SHADER_STORAGE_BUFFER, cc.scratch_param);
        (dispatch.buffer_data)(
            GL_SHADER_STORAGE_BUFFER,
            4,
            &raw as *const u32 as *const std::ffi::c_void,
            GL_DYNAMIC_DRAW,
        );
        (dispatch.get_buffer_parameter_iv)(
            GL_SHADER_STORAGE_BUFFER,
            GL_BUFFER_SIZE,
            &mut param_size,
        );
        (dispatch.bind_buffer)(GL_SHADER_STORAGE_BUFFER, prev_ssbo as u32);
    }
    if param_size < 4 {
        log::warn!("[FluorateGL] IndirectCount compaction: 参数 buffer 分配失败，走 CPU 路径");
        return false;
    }

    // dst 分配（MG：无条件 realloc——防上一帧 draw 与本 dispatch 竞争，勿改 SubData）
    let dst_bytes = maxdrawcount as usize * cmd_bytes as usize;
    let mut dst_size = 0i32;
    unsafe {
        (dispatch.bind_buffer)(GL_SHADER_STORAGE_BUFFER, cc.scratch_dst);
        (dispatch.buffer_data)(
            GL_SHADER_STORAGE_BUFFER,
            dst_bytes as isize,
            std::ptr::null(),
            GL_DYNAMIC_DRAW,
        );
        (dispatch.get_buffer_parameter_iv)(GL_SHADER_STORAGE_BUFFER, GL_BUFFER_SIZE, &mut dst_size);
        (dispatch.bind_buffer)(GL_SHADER_STORAGE_BUFFER, prev_ssbo as u32);
    }
    if dst_size < 0 || (dst_size as usize) < dst_bytes {
        log::warn!(
            "[FluorateGL] IndirectCount compaction: 命令输出 buffer 分配失败（要 {} 得 {}），走 CPU 路径",
            dst_bytes,
            dst_size
        );
        return false;
    }

    // bind SSBO + program + uniform（uCountWord 恒 0：count 在 scratch_param[0]）
    unsafe {
        (dispatch.bind_buffer_base)(GL_SHADER_STORAGE_BUFFER, 0, prev_indirect);
        (dispatch.bind_buffer_base)(GL_SHADER_STORAGE_BUFFER, 1, cc.scratch_param);
        (dispatch.bind_buffer_base)(GL_SHADER_STORAGE_BUFFER, 2, cc.scratch_dst);
        (dispatch.use_program)(cc.program);
        (dispatch.uniform_1ui)(cc.loc_max, maxdrawcount as u32);
        (dispatch.uniform_1ui)(cc.loc_srcwords, (src_stride / 4) as u32);
        (dispatch.uniform_1ui)(cc.loc_srcoff, (src_off / 4) as u32);
        (dispatch.uniform_1ui)(cc.loc_cntoff, 0);
        (dispatch.uniform_1ui)(cc.loc_dstwords, (cmd_bytes / 4) as u32);
    }

    // work group 上限（GLES 3.1 保证 ≥65535；MG 查询 + 校验）
    let mut max_groups = 0i32;
    unsafe { (dispatch.get_integeri_v)(GL_MAX_COMPUTE_WORK_GROUP_COUNT, 0, &mut max_groups) }
    if max_groups <= 0 {
        max_groups = 65535;
    }
    let groups = ((maxdrawcount as u64) + 63) / 64;
    if groups > max_groups as u64 {
        log::warn!(
            "[FluorateGL] IndirectCount compaction: work group 数 {} 超限 {}，走 CPU 路径",
            groups,
            max_groups
        );
        restore_ssbo(dispatch);
        unsafe { (dispatch.use_program)(prev_program as u32) }
        return false;
    }

    // 探针 dispatch（MG mg_md_drain + mg_md_check：先排空队列再检查，防止
    // app 遗留错误被误判为本层失败）
    unsafe {
        for _ in 0..16 {
            if (dispatch.get_error)() == GL_NO_ERROR {
                break;
            }
        }
        (dispatch.dispatch_compute)(groups as u32, 1, 1);
    }
    let err = unsafe { (dispatch.get_error)() };
    if err != GL_NO_ERROR {
        log::warn!(
            "[FluorateGL] IndirectCount compaction: dispatch 失败 0x{:04X}，走 CPU 路径",
            err
        );
        restore_ssbo(dispatch);
        unsafe { (dispatch.use_program)(prev_program as u32) }
        return false;
    }

    unsafe {
        (dispatch.memory_barrier)(GL_SHADER_STORAGE_BARRIER_BIT | GL_COMMAND_BARRIER_BIT);
    }

    // 恢复 SSBO/program 后再 draw（MG：scratch bindings 不能留给 app 的 shader）
    restore_ssbo(dispatch);
    unsafe { (dispatch.use_program)(prev_program as u32) }

    // 循环 indirect draw（无 EXT 批量符号，MG fallback 分支同款——越界命令
    // instanceCount=0，逐条画结果一致）
    unsafe {
        (dispatch.bind_buffer)(GL_DRAW_INDIRECT_BUFFER, cc.scratch_dst);
        for i in 0..maxdrawcount as isize {
            let off = (i * cmd_bytes) as *const std::ffi::c_void;
            if is_elements {
                (dispatch.draw_elements_indirect)(mode, type_, off);
            } else {
                (dispatch.draw_arrays_indirect)(mode, off);
            }
        }
        (dispatch.bind_buffer)(GL_DRAW_INDIRECT_BUFFER, prev_indirect);
    }
    true
}
