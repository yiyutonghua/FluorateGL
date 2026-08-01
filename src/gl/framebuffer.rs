use crate::backend;
use crate::state;
use log;
use std::sync::atomic::{AtomicBool, Ordering};

/// FBO 相关资源（framebuffer / renderbuffer / 附件纹理）desktop ID 在 IdMap 中
/// 查找失败时的首次告警标志。跨线程绑定或资源已释放时会触发，避免每帧刷屏。
static FBO_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：FBO 相关资源 desktop ID 未在 IdMap 中找到。
///
/// 触发场景：跨线程绑定（异步加载线程访问 GL）、资源已被释放但上层仍持有旧 ID。
/// 后续调用将静默 unbinding（传 0 给 GLES），不影响其他正常资源的绑定。
fn warn_fbo_id_miss(fname: &str, target: u32, desktop_id: u32) {
    if !FBO_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: target 0x{:04X} desktop ID {} not found in IdMap, unbinding (跨线程或资源已释放，后续将静默降级)",
            fname,
            target,
            desktop_id
        );
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenFramebuffers(n: i32, framebuffers: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let mut gles_id = 0u32;
            (dispatch.gen_framebuffers)(1, &mut gles_id);

            let desktop_id = state::with_state(|s| s.framebuffers.alloc(gles_id));
            *framebuffers.offset(i) = desktop_id;
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteFramebuffers(n: i32, framebuffers: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let desktop_id = *framebuffers.offset(i);
            let gles_id = state::with_state(|s| {
                if s.bound_framebuffer == desktop_id {
                    s.bound_framebuffer = 0;
                }
                s.framebuffers.delete(desktop_id)
            });
            if let Some(gles_id) = gles_id {
                (dispatch.delete_framebuffers)(1, &gles_id);
            }
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindFramebuffer(target: u32, framebuffer: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if framebuffer == 0 {
            (dispatch.bind_framebuffer)(target, 0);
            state::with_state(|s| s.bound_framebuffer = 0);
        } else if let Some(gles_id) = state::with_state(|s| s.framebuffers.get_gles(framebuffer)) {
            (dispatch.bind_framebuffer)(target, gles_id);
            state::with_state(|s| s.bound_framebuffer = framebuffer);
        } else {
            warn_fbo_id_miss("glBindFramebuffer", target, framebuffer);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFramebufferTexture2D(
    target: u32,
    attachment: u32,
    textarget: u32,
    texture: u32,
    level: i32,
) {
    log::debug!(
        "[FluorateGL] glFramebufferTexture2D(target=0x{:04X}, attachment=0x{:04X}, textarget=0x{:04X}, texture={}, level={})",
        target,
        attachment,
        textarget,
        texture,
        level
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_texture = if texture == 0 {
            0
        } else {
            state::with_state(|s| {
                s.textures.get_gles(texture).unwrap_or_else(|| {
                    warn_fbo_id_miss("glFramebufferTexture2D", target, texture);
                    0
                })
            })
        };

        (dispatch.framebuffer_texture_2d)(target, attachment, textarget, gles_texture, level);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFramebufferTextureLayer(
    target: u32,
    attachment: u32,
    texture: u32,
    level: i32,
    layer: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_texture = if texture == 0 {
            0
        } else {
            state::with_state(|s| {
                s.textures.get_gles(texture).unwrap_or_else(|| {
                    warn_fbo_id_miss("glFramebufferTextureLayer", target, texture);
                    0
                })
            })
        };

        (dispatch.framebuffer_texture_layer)(target, attachment, gles_texture, level, layer);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFramebufferRenderbuffer(
    target: u32,
    attachment: u32,
    renderbuffertarget: u32,
    renderbuffer: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_renderbuffer = if renderbuffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.renderbuffers.get_gles(renderbuffer).unwrap_or_else(|| {
                    warn_fbo_id_miss("glFramebufferRenderbuffer", target, renderbuffer);
                    0
                })
            })
        };

        (dispatch.framebuffer_renderbuffer)(
            target,
            attachment,
            renderbuffertarget,
            gles_renderbuffer,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCheckFramebufferStatus(target: u32) -> u32 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.check_framebuffer_status)(target) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenRenderbuffers(n: i32, renderbuffers: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let mut gles_id = 0u32;
            (dispatch.gen_renderbuffers)(1, &mut gles_id);

            let desktop_id = state::with_state(|s| s.renderbuffers.alloc(gles_id));
            *renderbuffers.offset(i) = desktop_id;
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteRenderbuffers(n: i32, renderbuffers: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let desktop_id = *renderbuffers.offset(i);
            let gles_id = state::with_state(|s| {
                if s.bound_renderbuffer == desktop_id {
                    s.bound_renderbuffer = 0;
                }
                s.renderbuffers.delete(desktop_id)
            });
            if let Some(gles_id) = gles_id {
                (dispatch.delete_renderbuffers)(1, &gles_id);
            }
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindRenderbuffer(target: u32, renderbuffer: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if renderbuffer == 0 {
            (dispatch.bind_renderbuffer)(target, 0);
            state::with_state(|s| s.bound_renderbuffer = 0);
        } else if let Some(gles_id) = state::with_state(|s| s.renderbuffers.get_gles(renderbuffer))
        {
            (dispatch.bind_renderbuffer)(target, gles_id);
            state::with_state(|s| s.bound_renderbuffer = renderbuffer);
        } else {
            warn_fbo_id_miss("glBindRenderbuffer", target, renderbuffer);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glRenderbufferStorage(target: u32, internalformat: u32, width: i32, height: i32) {
    log::debug!(
        "[FluorateGL] glRenderbufferStorage(target=0x{:04X}, internalformat=0x{:04X}, {}x{})",
        target,
        internalformat,
        width,
        height
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.renderbuffer_storage)(target, internalformat, width, height);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glRenderbufferStorageMultisample(
    target: u32,
    samples: i32,
    internalformat: u32,
    width: i32,
    height: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.renderbuffer_storage_multisample)(target, samples, internalformat, width, height);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlitFramebuffer(
    srcX0: i32,
    srcY0: i32,
    srcX1: i32,
    srcY1: i32,
    dstX0: i32,
    dstY0: i32,
    dstX1: i32,
    dstY1: i32,
    mask: u32,
    filter: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blit_framebuffer)(
            srcX0, srcY0, srcX1, srcY1, dstX0, dstY0, dstX1, dstY1, mask, filter,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawBuffers(n: i32, bufs: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_buffers)(n, bufs);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glReadBuffer(mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.read_buffer)(mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glReadPixels(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    format: u32,
    type_: u32,
    pixels: *mut std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.read_pixels)(x, y, width, height, format, type_, pixels);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearBufferfv(buffer: u32, drawbuffer: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_buffer_fv)(buffer, drawbuffer, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearBufferiv(buffer: u32, drawbuffer: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_buffer_iv)(buffer, drawbuffer, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearBufferuiv(buffer: u32, drawbuffer: i32, value: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_buffer_uiv)(buffer, drawbuffer, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearBufferfi(buffer: u32, drawbuffer: i32, depth: f32, stencil: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_buffer_fi)(buffer, drawbuffer, depth, stencil);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFramebufferAttachmentParameteriv(
    target: u32,
    attachment: u32,
    pname: u32,
    params: *mut i32,
) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_framebuffer_attachment_parameter_iv)(target, attachment, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsFramebuffer(framebuffer: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if framebuffer == 0 {
            0
        } else {
            state::with_state(|s| s.framebuffers.get_gles(framebuffer).unwrap_or(0))
        };

        (dispatch.is_framebuffer)(gles_id)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsRenderbuffer(renderbuffer: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if renderbuffer == 0 {
            0
        } else {
            state::with_state(|s| s.renderbuffers.get_gles(renderbuffer).unwrap_or(0))
        };

        (dispatch.is_renderbuffer)(gles_id)
    })
}
