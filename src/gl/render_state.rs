use crate::backend;
use crate::gl::exports::is_unsupported_gles_cap;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEnablei(cap: u32, index: u32) {
    if is_unsupported_gles_cap(cap) {
        log::info!(
            "[FluorateGL] glEnablei(0x{:04X}, {}) ignored (unsupported in GLES)",
            cap, index
        );
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.enable_i)(cap, index);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDisablei(cap: u32, index: u32) {
    if is_unsupported_gles_cap(cap) {
        log::info!(
            "[FluorateGL] glDisablei(0x{:04X}, {}) ignored (unsupported in GLES)",
            cap, index
        );
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.disable_i)(cap, index);
    });
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

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPolygonMode(face: u32, mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.polygon_mode)(face, mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPixelStoref(pname: u32, param: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.pixel_store_f)(pname, param);
    });
}
