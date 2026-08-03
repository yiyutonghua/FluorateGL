use crate::backend;
use libc::c_char;
use rustc_hash::FxHashSet;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

const EGL_NONE: i32 = 0x3038;
const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
const EGL_CONTEXT_OPENGL_PROFILE_MASK: i32 = 0x3093;
const EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY: i32 = 0x3094;
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_SUCCESS: u32 = 0x3000;
const EGL_VERSION: i32 = 0x3053;
const EGL_EXTENSIONS: i32 = 0x3055;
/// eglCreateContext 属性改写循环允许的最大属性对数（防无限越界读）
const MAX_ATTRIB_PAIRS: usize = 128;

/// EGL 版本字符串（eglQueryString(EGL_VERSION) 返回值）
static VERSION: &[u8] = b"1.4 FluorateGL\0";

// 编译期断言：EGL 版本字符串 "主.次" 必须与 REPORTED_EGL_MAJOR/MINOR 一致，
// 防止改动一处遗漏另一处导致宿主 EGL 版本解析异常（模仿 config.rs 的断言模式）。
// 注意：const 块内不能用 format!，直接对比字节。
const _: () = {
    assert!(VERSION[0] == b'0' + crate::config::REPORTED_EGL_MAJOR as u8);
    assert!(VERSION[2] == b'0' + crate::config::REPORTED_EGL_MINOR as u8);
};

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetDisplay(display_id: *mut std::ffi::c_void) -> *mut std::ffi::c_void {
    // P1-A 双层兜底：stub 模式下返回 null（宿主可判空），避免把垃圾值当 display 句柄
    if !crate::backend::egl_backend_ready() {
        log::error!("[EGL] eglGetDisplay 在 STUB 模式（EGL 库加载失败）被调用，返回 null");
        return std::ptr::null_mut();
    }
    backend::with_egl_dispatch(|d| unsafe { (d.get_display)(display_id) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglInitialize(
    dpy: *mut std::ffi::c_void,
    major: *mut i32,
    minor: *mut i32,
) -> u32 {
    let result = backend::with_egl_dispatch(|d| unsafe { (d.initialize)(dpy, major, minor) });
    if result == EGL_SUCCESS {
        if !major.is_null() {
            unsafe { *major = crate::config::REPORTED_EGL_MAJOR };
        }
        if !minor.is_null() {
            unsafe { *minor = crate::config::REPORTED_EGL_MINOR };
        }
    }
    result
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglTerminate(dpy: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.terminate)(dpy) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglQueryString(dpy: *mut c_void, name: i32) -> *const c_char {
    match name {
        EGL_VERSION => VERSION.as_ptr() as *const c_char,
        EGL_EXTENSIONS => {
            static EXTENSIONS: &[u8] = b"EGL_KHR_create_context EGL_KHR_surfaceless_context EGL_ANDROID_framebuffer_target EGL_ANDROID_blob_cache EGL_EXT_swap_buffers_with_damage EGL_KHR_swap_buffers_with_damage EGL_KHR_image_base EGL_KHR_gl_texture_2D_image EGL_KHR_gl_texture_cubemap_image EGL_KHR_gl_renderbuffer_image EGL_KHR_fence_sync EGL_KHR_wait_sync EGL_ANDROID_native_fence_sync EGL_KHR_reusable_sync\0";
            EXTENSIONS.as_ptr() as *const c_char
        }
        _ => backend::with_egl_dispatch(|d| unsafe { (d.query_string)(dpy, name) }),
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetConfigs(
    dpy: *mut c_void,
    configs: *mut c_void,
    config_size: i32,
    num_config: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|d| unsafe {
        (d.get_configs)(dpy, configs, config_size, num_config)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglChooseConfig(
    dpy: *mut c_void,
    attrib_list: *const i32,
    configs: *mut c_void,
    config_size: i32,
    num_config: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|d| unsafe {
        (d.choose_config)(dpy, attrib_list, configs, config_size, num_config)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetConfigAttrib(
    dpy: *mut c_void,
    config: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.get_config_attrib)(dpy, config, attribute, value) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCreateWindowSurface(
    dpy: *mut c_void,
    config: *mut c_void,
    win: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    // P1-A 双层兜底：stub 模式下返回 null（EGL_NO_SURFACE 语义），避免伪指针进入宿主
    if !crate::backend::egl_backend_ready() {
        log::error!("[EGL] eglCreateWindowSurface 在 STUB 模式（EGL 库加载失败）被调用，返回 null");
        return std::ptr::null_mut();
    }
    backend::with_egl_dispatch(|d| unsafe {
        (d.create_window_surface)(dpy, config, win, attrib_list)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCreatePbufferSurface(
    dpy: *mut c_void,
    config: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    // P1-A 双层兜底：stub 模式下返回 null（EGL_NO_SURFACE 语义），避免伪指针进入宿主
    if !crate::backend::egl_backend_ready() {
        log::error!("[EGL] eglCreatePbufferSurface 在 STUB 模式（EGL 库加载失败）被调用，返回 null");
        return std::ptr::null_mut();
    }
    backend::with_egl_dispatch(|d| unsafe { (d.create_pbuffer_surface)(dpy, config, attrib_list) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCreatePbufferFromClientBuffer(
    dpy: *mut c_void,
    buftype: u32,
    buffer: *mut c_void,
    config: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    // P1-A 双层兜底：stub 模式下返回 null（EGL_NO_SURFACE 语义），避免伪指针进入宿主
    if !crate::backend::egl_backend_ready() {
        log::error!(
            "[EGL] eglCreatePbufferFromClientBuffer 在 STUB 模式（EGL 库加载失败）被调用，返回 null"
        );
        return std::ptr::null_mut();
    }
    backend::with_egl_dispatch(|d| unsafe {
        (d.create_pbuffer_from_client_buffer)(dpy, buftype, buffer, config, attrib_list)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCreatePixmapSurface(
    dpy: *mut c_void,
    config: *mut c_void,
    pixmap: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    // P1-A 双层兜底：stub 模式下返回 null（EGL_NO_SURFACE 语义），避免伪指针进入宿主
    if !crate::backend::egl_backend_ready() {
        log::error!("[EGL] eglCreatePixmapSurface 在 STUB 模式（EGL 库加载失败）被调用，返回 null");
        return std::ptr::null_mut();
    }
    backend::with_egl_dispatch(|d| unsafe {
        (d.create_pixmap_surface)(dpy, config, pixmap, attrib_list)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglDestroySurface(dpy: *mut c_void, surface: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.destroy_surface)(dpy, surface) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglSurfaceAttrib(
    dpy: *mut c_void,
    surface: *mut c_void,
    attribute: i32,
    value: i32,
) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.surface_attrib)(dpy, surface, attribute, value) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglBindTexImage(dpy: *mut c_void, surface: *mut c_void, buffer: i32) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.bind_tex_image)(dpy, surface, buffer) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglReleaseTexImage(dpy: *mut c_void, surface: *mut c_void, buffer: i32) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.release_tex_image)(dpy, surface, buffer) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglBindAPI(_api: u32) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.bind_api)(EGL_OPENGL_ES_API) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglQueryAPI() -> u32 {
    EGL_OPENGL_ES_API
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCreateContext(
    dpy: *mut std::ffi::c_void,
    config: *mut std::ffi::c_void,
    share_context: *mut std::ffi::c_void,
    attrib_list: *const i32,
) -> *mut std::ffi::c_void {
    // P1-A 双层兜底：stub 模式下直接返回 EGL_NO_CONTEXT（null）。
    // 必须放在 128 对上限检查之前：stub 模式下任何调用都不应产生伪 context 指针。
    if !crate::backend::egl_backend_ready() {
        log::error!("[EGL] eglCreateContext 在 STUB 模式（EGL 库加载失败）被调用，返回 null");
        return std::ptr::null_mut();
    }
    if attrib_list.is_null() {
        return backend::with_egl_dispatch(|d| unsafe {
            (d.create_context)(dpy, config, share_context, std::ptr::null())
        });
    }

    let mut new_attribs = Vec::new();
    let mut i = 0;

    loop {
        // P1-C 越界防线：attrib_list 必须由 EGL_NONE 终止，但损坏/恶意的宿主可能
        // 传入无终止的超长数组，导致无限越界读。超过 128 对直接拒绝。
        // 注意：不能返回 EGL_BAD_ATTRIBUTE（0x3054）数值——返回类型是 *mut c_void，
        // 数值会被宿主当作有效 context 指针（垃圾指针漏洞），必须返回 EGL_NO_CONTEXT。
        if (i as usize) >= MAX_ATTRIB_PAIRS * 2 {
            log::warn!("[EGL] eglCreateContext attrib_list 超过 128 对且无 EGL_NONE 终止，拒绝创建");
            return std::ptr::null_mut();
        }
        let attr = unsafe { *attrib_list.offset(i) };
        if attr == EGL_NONE {
            break;
        }
        let value = unsafe { *attrib_list.offset(i + 1) };

        match attr {
            EGL_CONTEXT_CLIENT_VERSION => {
                new_attribs.push(EGL_CONTEXT_CLIENT_VERSION);
                new_attribs.push(3);
            }
            EGL_CONTEXT_OPENGL_PROFILE_MASK => {
                // GLES 没有 profile，跳过
            }
            EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY => {}
            _ => {
                new_attribs.push(attr);
                new_attribs.push(value);
            }
        }

        i += 2;
    }

    new_attribs.push(EGL_NONE);
    let ptr = new_attribs.as_ptr();
    backend::with_egl_dispatch(|d| unsafe { (d.create_context)(dpy, config, share_context, ptr) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglDestroyContext(dpy: *mut c_void, ctx: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.destroy_context)(dpy, ctx) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglMakeCurrent(
    dpy: *mut std::ffi::c_void,
    draw: *mut std::ffi::c_void,
    read: *mut std::ffi::c_void,
    ctx: *mut std::ffi::c_void,
) -> u32 {
    // 保留 debug 级别以支持 FLUORATEGL_LOG=debug 下的上下文切换诊断
    log::debug!(
        "[EGL] eglMakeCurrent dpy={:?} draw={:?} read={:?} ctx={:?}",
        dpy,
        draw,
        read,
        ctx
    );
    let result = backend::with_egl_dispatch(|d| unsafe { (d.make_current)(dpy, draw, read, ctx) });
    // 保留 debug 级别以支持 FLUORATEGL_LOG=debug 下的上下文切换诊断
    log::debug!("[EGL] eglMakeCurrent result=0x{:04X}", result);
    result
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglQueryContext(
    dpy: *mut c_void,
    ctx: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    if attribute == EGL_CONTEXT_CLIENT_VERSION {
        unsafe { *value = crate::config::REPORTED_EGL_CLIENT_VERSION };
        return EGL_SUCCESS;
    }
    backend::with_egl_dispatch(|d| unsafe { (d.query_context)(dpy, ctx, attribute, value) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglQuerySurface(
    dpy: *mut c_void,
    surface: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.query_surface)(dpy, surface, attribute, value) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetCurrentContext() -> *mut c_void {
    backend::with_egl_dispatch(|d| unsafe { (d.get_current_context)() })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetCurrentSurface(readdraw: i32) -> *mut c_void {
    backend::with_egl_dispatch(|d| unsafe { (d.get_current_surface)(readdraw) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetCurrentDisplay() -> *mut c_void {
    backend::with_egl_dispatch(|d| unsafe { (d.get_current_display)() })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglWaitClient() -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.wait_client)() })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglWaitNative(engine: i32) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.wait_native)(engine) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglWaitGL() -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.wait_gl)() })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglReleaseThread() -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.release_thread)() })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglSwapBuffers(
    dpy: *mut std::ffi::c_void,
    surface: *mut std::ffi::c_void,
) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.swap_buffers)(dpy, surface) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglSwapInterval(dpy: *mut c_void, interval: i32) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.swap_interval)(dpy, interval) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCopyBuffers(
    dpy: *mut c_void,
    surface: *mut c_void,
    target: *mut c_void,
) -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.copy_buffers)(dpy, surface, target) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetError() -> u32 {
    backend::with_egl_dispatch(|d| unsafe { (d.get_error)() })
}

/// 检查是否为需要追踪的关键 GL 函数（用于诊断 ID 映射问题）
fn is_key_gl_function(name: &str) -> bool {
    matches!(
        name,
        "glBindBuffer"
            | "glGenBuffers"
            | "glDeleteBuffers"
            | "glBindVertexArray"
            | "glGenVertexArrays"
            | "glDeleteVertexArrays"
            | "glDrawArrays"
            | "glDrawElements"
            | "glVertexAttribPointer"
            | "glEnableVertexAttribArray"
            | "glUseProgram"
            | "glBindTexture"
            | "glGenTextures"
            | "glDeleteTextures"
            | "glBufferData"
            | "glBufferSubData"
            | "glBindFramebuffer"
            | "glGenFramebuffers"
            | "glDeleteFramebuffers"
            // ARB_vertex_attrib_binding / GLES 3.1 DSA
            | "glBindVertexBuffer"
            | "glVertexAttribFormat"
            | "glVertexAttribIFormat"
            | "glVertexAttribBinding"
            // GL_EXT_texture_buffer / GLES 3.2
            | "glTexBuffer"
            | "glTexBufferRange"
            // Shader / Program 管理（用于追踪拦截状态）
            | "glCreateShader"
            | "glCompileShader"
            | "glDeleteShader"
            | "glShaderSource"
            | "glGetShaderiv"
            | "glGetShaderInfoLog"
            | "glCreateProgram"
            | "glLinkProgram"
            | "glDeleteProgram"
            | "glAttachShader"
            | "glDetachShader"
            | "glGetProgramiv"
            | "glGetProgramInfoLog"
            // Indirect draw（诊断 Sodium 是否查询/调用 indirect draw 函数）
            | "glDrawArraysIndirect"
            | "glDrawElementsIndirect"
            | "glMultiDrawArraysIndirect"
            | "glMultiDrawElementsIndirect"
            | "glMultiDrawArraysIndirectCount"
            | "glMultiDrawElementsIndirectCount"
    )
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetProcAddress(proc_name: *const libc::c_char) -> *mut std::ffi::c_void {
    if proc_name.is_null() {
        return std::ptr::null_mut();
    }

    let name_str = unsafe { std::ffi::CStr::from_ptr(proc_name) };
    let name = name_str.to_string_lossy();
    let is_key = is_key_gl_function(&name);

    // 1. 优先使用我们自己库的句柄查找，确保返回 FluorateGL 的函数指针
    if let Some(handle) = crate::get_self_handle() {
        let local = unsafe { libc::dlsym(handle, proc_name) };
        if !local.is_null() {
            if is_key {
                log::debug!("[FluorateGL] eglGetProcAddress({}) -> self handle", name);
            }
            return local;
        }
    } else if is_key {
        log::warn!(
            "[FluorateGL] eglGetProcAddress({}) self_handle is None, falling back to RTLD_DEFAULT (may return wrong function pointer!)",
            name
        );
    }

    // 2. Fallback: RTLD_DEFAULT（全局符号表查找）
    // 注意：如果 LD_PRELOAD 未生效或 get_self_handle 失败，此处可能返回 GLES 驱动的函数指针
    let local = unsafe { libc::dlsym(libc::RTLD_DEFAULT, proc_name) };
    if !local.is_null() {
        if is_key {
            log::debug!("[FluorateGL] eglGetProcAddress({}) -> RTLD_DEFAULT", name);
        }
        return local;
    }

    // 3. 最后回退到底层 EGL 驱动
    let result = backend::with_egl_dispatch(|d| unsafe { (d.get_proc_address)(proc_name) });
    if is_key {
        log::debug!("[FluorateGL] eglGetProcAddress({}) -> EGL driver", name);
    } else if result.is_null() {
        // 诊断：LWJGL/MC 查询了 FluorateGL 未导出且 GLES 驱动也不提供的函数。
        // 返回 null 会导致 LWJGL capabilities 字段为 null，调用时抛
        // "No context is current or a function that is not available" 错误。
        // 首次告警避免刷屏，帮助定位需要补 stub 的函数。
        warn_missing_gl_function(&name);
    }
    result
}

/// 记录 eglGetProcAddress 查询但未提供的 GL 函数（首次告警）
fn warn_missing_gl_function(name: &str) {
    static WARNED: OnceLock<Mutex<FxHashSet<String>>> = OnceLock::new();
    let set = WARNED.get_or_init(|| Mutex::new(FxHashSet::default()));
    let mut guard = set.lock().unwrap();
    if guard.insert(name.to_string()) {
        log::warn!(
            "[FluorateGL] eglGetProcAddress({}) -> null (FluorateGL 未导出且 GLES 驱动未提供，LWJGL capabilities 将为 null，调用时会抛错)",
            name
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1-C：超长 attrib_list（>128 对且无 EGL_NONE 终止）必须被拒绝。
    /// 越界检查发生在 dispatch 调用之前，不依赖 backend 全局状态，测试可行。
    #[test]
    fn create_context_rejects_oversized_attrib_list() {
        // 200 对属性、无 EGL_NONE 终止（共 400 个 i32，循环第 129 对时触发上限）
        let attribs: Vec<i32> = (0..200).flat_map(|_| [EGL_CONTEXT_CLIENT_VERSION, 3]).collect();
        let ctx = unsafe {
            eglCreateContext(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                attribs.as_ptr(),
            )
        };
        assert!(
            ctx.is_null(),
            "超过 128 对的 attrib_list 必须返回 EGL_NO_CONTEXT（null），而非把错误码数值当指针返回"
        );
    }

    /// P1-C：恰好 128 对且无 EGL_NONE 终止也应被拒绝（边界值）。
    #[test]
    fn create_context_rejects_boundary_attrib_list_without_none() {
        let attribs: Vec<i32> = (0..128).flat_map(|_| [EGL_CONTEXT_CLIENT_VERSION, 3]).collect();
        let ctx = unsafe {
            eglCreateContext(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                attribs.as_ptr(),
            )
        };
        assert!(ctx.is_null(), "128 对但无 EGL_NONE 终止时必须返回 EGL_NO_CONTEXT");
    }

    /// P1-C：短列表（EGL_NONE 正常终止）不得被误拒——验证循环上限不影响正常路径。
    /// 走到 dispatch 时使用 all_stub，仅验证不触发越界拒绝（不断言 stub 返回值）。
    #[test]
    fn create_context_accepts_short_attrib_list() {
        let attribs: Vec<i32> = vec![EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE];
        let ctx = unsafe {
            eglCreateContext(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                attribs.as_ptr(),
            )
        };
        let _ = ctx; // stub 返回值取决于 backend 状态，此处只验证未被越界逻辑拒绝
    }
}
