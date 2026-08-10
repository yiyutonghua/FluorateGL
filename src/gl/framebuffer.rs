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

/// M3：glGetFramebufferAttachmentParameteriv 的 OBJECT_NAME 回译失败首次告警标志。
/// GLES 返回的原生纹理/RB ID 无对应 desktop ID（跨线程或资源已释放）时触发一次。
static OBJECT_NAME_TRANSLATE_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：OBJECT_NAME 回译失败，写 0 给宿主（宿主无法用该 ID 操作对象）。
fn warn_object_name_translate_miss(attachment: u32, gles_id: u32) {
    if !OBJECT_NAME_TRANSLATE_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glGetFramebufferAttachmentParameteriv(OBJECT_NAME, attachment=0x{:04X}): GLES ID {} 无对应 desktop ID，写 0 (跨线程或资源已释放，后续将静默降级)",
            attachment,
            gles_id
        );
    }
}

/// C5：RenderbufferStorage 的 RGB 浮点格式降级（→RGBA 变体）首次告警标志。
/// GLES 无可渲染的 RGB 浮点格式，RGB32F/RGB16F 降级后透明度恒为 1，RGB 语义等价。
static RENDERBUFFER_FORMAT_DOWNGRADE_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：RGB 浮点内部格式已降级为 RGBA 变体。
fn warn_renderbuffer_format_downgrade(original: u32, mapped: u32) {
    if !RENDERBUFFER_FORMAT_DOWNGRADE_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glRenderbufferStorage: GLES 无 RGB 浮点可渲染格式，internalformat 0x{:04X} -> 0x{:04X} (首次转换后静默)",
            original,
            mapped
        );
    }
}

/// glFramebufferTexture 在 GLES 3.1 驱动（无 glFramebufferTexture 符号）下降级为
/// glFramebufferTextureLayer(layer=0) 的首次告警标志。
/// 降级语义：2D 纹理完全等价（层 0 即全部）；3D/array 纹理仅附加 layer 0
/// （GL 的 glFramebufferTexture 附加整个纹理对象，属接受局限）。
static FBT_LAYER_DOWNGRADE_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glFramebufferTexture 已降级为 glFramebufferTextureLayer(layer=0)。
fn warn_framebuffer_texture_downgrade() {
    if !FBT_LAYER_DOWNGRADE_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glFramebufferTexture: GLES 驱动无 glFramebufferTexture（需 GLES 3.2），降级为 glFramebufferTextureLayer(layer=0) —— 2D 纹理等价，3D/array 纹理仅附加 layer 0 (首次降级后静默)"
        );
    }
}

/// dispatch 函数指针是否为 stub（驱动未导出该符号）。
fn is_stub(dispatch: &backend::dispatch::GlesDispatch, f: *const ()) -> bool {
    f == dispatch.stub as *const ()
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

/// glFramebufferTexture — 桌面 GL_FramebufferTexture（GL 3.2，GL_EXT_framebuffer_texture
/// 家族），GLES 3.2 core 同名原生函数（MG framebuffer.cpp 透传）。
///
/// 语义：把整个纹理对象（TextureAll）附加到 `attachment`（纹理层级 0、mip `level`），
/// 与 glFramebufferTexture2D（仅 2D 单层）和 glFramebufferTextureLayer（仅单层）不同——
/// 用于 cube/3D/array 纹理整体附加。
///
/// 降级链（对齐 MG 行为 + 驱动能力检测）：
/// 1. dispatch.framebuffer_texture 非 stub（GLES 3.2+ 驱动）→ 纹理 ID 翻译 + 透传；
/// 2. stub（GLES 3.1 及以下驱动无此符号）→ 降级 glFramebufferTextureLayer(layer=0)：
///    2D 纹理完全等价（层 0 即全部内容）；3D/array 纹理仅附加 layer 0（接受局限，
///    首调告警，后续静默）。GLES 3.1 无 glFramebufferTexture 的替代语义，不降级
///    则调用方（LWJGL capabilities 绑定 null 防护）在 3.1 驱动上拿到空函数崩溃。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFramebufferTexture(target: u32, attachment: u32, texture: u32, level: i32) {
    log::debug!(
        "[FluorateGL] glFramebufferTexture(target=0x{:04X}, attachment=0x{:04X}, texture={}, level={})",
        target,
        attachment,
        texture,
        level
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_texture = if texture == 0 {
            0
        } else {
            state::with_state(|s| {
                s.textures.get_gles(texture).unwrap_or_else(|| {
                    warn_fbo_id_miss("glFramebufferTexture", target, texture);
                    0
                })
            })
        };

        if is_stub(dispatch, dispatch.framebuffer_texture as *const ()) {
            // GLES 3.1 降级：glFramebufferTextureLayer(target, attachment, tex, level, 0)
            warn_framebuffer_texture_downgrade();
            (dispatch.framebuffer_texture_layer)(target, attachment, gles_texture, level, 0);
        } else {
            (dispatch.framebuffer_texture)(target, attachment, gles_texture, level);
        }
    });
}

/// glDeleteFramebuffersARB — GL_EXT_framebuffer_object 的 ARB 后缀别名
/// （MG framebuffer.cpp 用 `alias("glDeleteFramebuffers")` 直接等价）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteFramebuffersARB(n: i32, framebuffers: *const u32) {
    glDeleteFramebuffers(n, framebuffers);
}

/// glFramebufferRenderbufferARB — GL_EXT_framebuffer_object 的 ARB 后缀别名
/// （MG framebuffer.cpp 用 `alias("glFramebufferRenderbuffer")` 直接等价）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFramebufferRenderbufferARB(
    target: u32,
    attachment: u32,
    renderbuffertarget: u32,
    renderbuffer: u32,
) {
    glFramebufferRenderbuffer(target, attachment, renderbuffertarget, renderbuffer);
}

/// glFramebufferTextureLayerARB — GL_EXT_framebuffer_texture 的 ARB 后缀别名
/// （MG framebuffer.cpp 用 `alias("glFramebufferTextureLayer")` 直接等价）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFramebufferTextureLayerARB(
    target: u32,
    attachment: u32,
    texture: u32,
    level: i32,
    layer: i32,
) {
    glFramebufferTextureLayer(target, attachment, texture, level, layer);
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
    // C5：GLES 无可渲染的 RGB 浮点格式（GL_EXT_color_buffer_float 仅覆盖 RGBA/RG/R 变体），
    // GL_RGB32F/GL_RGB16F 直通会产生 GL_INVALID_ENUM。降级为 RGBA 变体：
    // 渲染时 RGB 通道数据逐通道拷贝，A 恒为 1，RGB 语义完全等价。
    // RGBA32F/RGBA16F 等其余格式直通。
    let mapped = match internalformat {
        0x8815 /* GL_RGB32F */ => 0x8814 /* GL_RGBA32F */,
        0x881B /* GL_RGB16F */ => 0x881A /* GL_RGBA16F */,
        other => other,
    };
    if mapped != internalformat {
        warn_renderbuffer_format_downgrade(internalformat, mapped);
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.renderbuffer_storage)(target, mapped, width, height);
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

/// glDrawBuffer(mode) — 桌面 GL 单缓冲区版本，GLES 无此函数。
///
/// 语义等价于 glDrawBuffers(1, &mode)。GLES 3.0+ FBO 默认绘制到
/// GL_COLOR_ATTACHMENT0，调用此函数通常是无副作用的安全操作。
///
/// 实现：转发到 glDrawBuffers。避免 LWJGL capabilities 字段为 null，
/// 导致 MC/OptiFine 设置 FBO 绘制缓冲区时抛 "No context is current" 错误，
/// 进而触发 Adreno 驱动 "Packing allocations" 性能警告刷屏。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawBuffer(mode: u32) {
    log::debug!(
        "[FluorateGL] glDrawBuffer(mode=0x{:04X}) -> glDrawBuffers(1, ...)",
        mode
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_buffers)(1, &mode);
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

        // M3：GL_FRAMEBUFFER_ATTACHMENT_OBJECT_NAME 回译——GLES 返回的是原生纹理/RB ID，
        // 宿主会当 desktop ID 用于后续操作（如 glDeleteTextures/glDeleteRenderbuffers），
        // 必须先经 IdMap 回译。先查 OBJECT_TYPE 区分对象种类；查不到时告警并写 0。
        if pname == 0x8CD1
        /* GL_FRAMEBUFFER_ATTACHMENT_OBJECT_NAME */
        {
            let mut obj_type: i32 = 0;
            (dispatch.get_framebuffer_attachment_parameter_iv)(
                target,
                attachment,
                0x8CD0, /* GL_FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE */
                &mut obj_type,
            );
            let gles_id = *params as u32;
            if gles_id != 0 {
                let desktop_id = match obj_type as u32 {
                    0x1702 /* GL_TEXTURE */ => {
                        state::with_state(|s| s.textures.get_desktop(gles_id))
                    }
                    0x8D41 /* GL_RENDERBUFFER */ => {
                        state::with_state(|s| s.renderbuffers.get_desktop(gles_id))
                    }
                    _ => None, // GL_NONE(0) 或其他类型：保持原样
                };
                match desktop_id {
                    Some(did) => *params = did as i32,
                    None => {
                        warn_object_name_translate_miss(attachment, gles_id);
                        *params = 0;
                    }
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dispatch::GlesDispatch;

    /// glFramebufferTexture 降级链前提：all_stub dispatch 的 framebuffer_texture
    /// 槽位必须能被 is_stub 识别为 stub（GLES 3.1 驱动缺 glFramebufferTexture 时
    /// 走 glFramebufferTextureLayer 降级），否则会调用不可用指针。
    #[test]
    fn is_stub_detects_all_stub_framebuffer_texture() {
        let d = GlesDispatch::all_stub();
        assert!(is_stub(&d, d.framebuffer_texture as *const ()));
    }
}
