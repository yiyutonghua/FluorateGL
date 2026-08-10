use crate::backend;
use std::sync::atomic::{AtomicBool, Ordering};

/// glPolygonMode 非 FILL 模式首次告警标志
static POLYGON_MODE_NON_FILL_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glPolygonMode 非 FILL 模式被忽略。
fn warn_polygon_mode_non_fill(face: u32, mode: u32) {
    if !POLYGON_MODE_NON_FILL_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glPolygonMode(0x{:04X}, 0x{:04X}): GLES 仅支持 GL_FILL，非 FILL 模式已忽略 (后续调用将静默跳过)",
            face,
            mode
        );
    }
}

/// 判断 dispatch 函数指针是否为共享的未实现 stub。
///
/// `load_opt!` 把缺失的可选函数替换为同一个 stub 函数，故 GlesDispatch 中所有 stub
/// 字段地址相同。与 `dispatch.stub` 比较即可判定该 GLES 函数是否被驱动支持。
fn is_stub(dispatch: &backend::dispatch::GlesDispatch, ptr: *const ()) -> bool {
    ptr == dispatch.stub as *const ()
}

/// glEnablei — 虚拟 enable 表 indexed 路径（移植 MobileGlues enable.cpp）。
///
/// 语义：GL 4.6 只有 GL_BLEND（按 draw buffer）与 GL_SCISSOR_TEST（按
/// viewport）两个 indexed cap；其余 cap 首次告警并忽略（MG EN_WARN_ONCE）。
/// 状态由 exports::enable_state 表持有，与 glIsEnabledi 回答一致。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEnablei(cap: u32, index: u32) {
    crate::gl::exports::enable_state::gl_enable_i(cap, index);
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDisablei(cap: u32, index: u32) {
    crate::gl::exports::enable_state::gl_disable_i(cap, index);
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendFunci(buf: u32, sfactor: u32, dfactor: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_func_i)(buf, sfactor, dfactor);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendFuncSeparate(srcRGB: u32, dstRGB: u32, srcAlpha: u32, dstAlpha: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_func_separate)(srcRGB, dstRGB, srcAlpha, dstAlpha);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendFuncSeparatei(
    buf: u32,
    srcRGB: u32,
    dstRGB: u32,
    srcAlpha: u32,
    dstAlpha: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_func_separate_i)(buf, srcRGB, dstRGB, srcAlpha, dstAlpha);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendEquation(mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_equation)(mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendEquationi(buf: u32, mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_equation_i)(buf, mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendEquationSeparate(modeRGB: u32, modeAlpha: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_equation_separate)(modeRGB, modeAlpha);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendEquationSeparatei(buf: u32, modeRGB: u32, modeAlpha: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_equation_separate_i)(buf, modeRGB, modeAlpha);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glColorMask(red: u8, green: u8, blue: u8, alpha: u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.color_mask)(red, green, blue, alpha);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glColorMaski(buf: u32, red: u8, green: u8, blue: u8, alpha: u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.color_mask_i)(buf, red, green, blue, alpha);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDepthRange(near: f64, far: f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // GLES 没有 glDepthRange(f64, f64)，统一走 glDepthRangef(f32, f32)
        (dispatch.depth_range_f)(near as f32, far as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDepthRangef(near: f32, far: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.depth_range_f)(near, far);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glStencilFunc(func: u32, ref_: i32, mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.stencil_func)(func, ref_, mask);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glStencilFuncSeparate(face: u32, func: u32, ref_: i32, mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.stencil_func_separate)(face, func, ref_, mask);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glStencilOp(fail: u32, zfail: u32, zpass: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.stencil_op)(fail, zfail, zpass);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glStencilOpSeparate(face: u32, fail: u32, zfail: u32, zpass: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.stencil_op_separate)(face, fail, zfail, zpass);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glStencilMask(mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.stencil_mask)(mask);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glStencilMaskSeparate(face: u32, mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.stencil_mask_separate)(face, mask);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPolygonOffset(factor: f32, units: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.polygon_offset)(factor, units);
    });
}

// GL_FILL = 0x1B02（GLES 唯一支持的光栅化模式）；GL_LINE(0x1B01)/GL_POINT(0x1B00) 不支持
const GL_FILL: u32 = 0x1B02;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPolygonMode(face: u32, mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.polygon_mode as *const ()) {
            // GLES 不支持 glPolygonMode，光栅化模式固定为 GL_FILL。
            // GL_LINE/GL_POINT 需 geometry shader 才能模拟，翻译层无法实现，仅告警并忽略。
            if mode != GL_FILL {
                warn_polygon_mode_non_fill(face, mode);
            }
            return;
        }
        (dispatch.polygon_mode)(face, mode);
    });
}

/// glPixelStoref — 移植 MG texture.cpp:2025-2033 语义。
///
/// 与整数形式设置同一份状态：GL 4.6 sec. 8.4.1 规定此形式对整数值参数
/// 四舍五入到最近整数（lroundf）。旧实现为 GLES 无 glPixelStoref 时
/// 直接截断转发；MG 语义更精确且共享 exports::glPixelStorei 的桌面
/// 6 参数影子存储（SWAP_BYTES 等 GLES 不认识的 pname 不再产生
/// INVALID_ENUM）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPixelStoref(pname: u32, param: f32) {
    crate::gl::exports::glPixelStorei(pname, param.round() as i32);
}

// === ARB 后缀别名 ===
//
// LWJGL 对 GL_ARB_draw_buffers_blend 扩展查询的是 ARB 后缀函数名，
// 故需额外导出 ARB 版本。这些函数与 core 版本（glBlendEquationi 等）
// 共享同一 GLES dispatch 实现，仅符号名不同；无需再做 ID 翻译或状态跟踪。

/// glBlendEquationiARB — GL_ARB_draw_buffers_blend 别名，转发到 core 版本。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendEquationiARB(buf: u32, mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_equation_i)(buf, mode);
    });
}

/// glBlendEquationSeparateiARB — GL_ARB_draw_buffers_blend 别名，转发到 core 版本。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendEquationSeparateiARB(buf: u32, modeRGB: u32, modeAlpha: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_equation_separate_i)(buf, modeRGB, modeAlpha);
    });
}

/// glBlendFunciARB — GL_ARB_draw_buffers_blend 别名，转发到 core 版本。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendFunciARB(buf: u32, src: u32, dst: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_func_i)(buf, src, dst);
    });
}

/// glBlendFuncSeparateiARB — GL_ARB_draw_buffers_blend 别名，转发到 core 版本。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendFuncSeparateiARB(
    buf: u32,
    srcRGB: u32,
    dstRGB: u32,
    srcAlpha: u32,
    dstAlpha: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_func_separate_i)(buf, srcRGB, dstRGB, srcAlpha, dstAlpha);
    });
}

// === GL Core no-op stub ===
//
// 以下函数为桌面 GL Core 特性，GLES 无对应实现，导出 no-op stub 避免
// LWJGL capabilities 字段为 null 导致调用时抛错。

/// glProvokingVertex stub — 桌面 GL 3.2 函数，GLES 无对应实现。
///
/// 语义：指定 provoking vertex（FIRST/_LAST）用于 flat shading 取顶点属性。
/// GLES 固定使用 LAST_VERTEX_PROVOKING，无法更改，no-op 实现安全。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glProvokingVertex(mode: u32) {
    log::debug!(
        "[FluorateGL] glProvokingVertex(0x{:04X}) -> no-op (GLES fixed to LAST_VERTEX_PROVOKING)",
        mode
    );
}

/// glBeginConditionalRender stub — 桌面 GL 3.0 函数，GLES 无对应实现。
///
/// 语义：基于 query object 结果条件渲染。GLES 不支持条件渲染，no-op 实现安全。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBeginConditionalRender(id: u32, mode: u32) {
    log::debug!(
        "[FluorateGL] glBeginConditionalRender(id={}, mode=0x{:04X}) -> no-op (GLES unsupported)",
        id,
        mode
    );
}

/// glEndConditionalRender stub — 桌面 GL 3.0 函数，GLES 无对应实现。
///
/// 语义：结束条件渲染区间。GLES 不支持条件渲染，no-op 实现安全。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEndConditionalRender() {
    log::debug!("[FluorateGL] glEndConditionalRender() -> no-op (GLES unsupported)");
}
