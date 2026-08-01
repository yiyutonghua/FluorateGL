use crate::backend;
use libc::c_char;
use std::ffi::c_void;

const EGL_NONE: i32 = 0x3038;
const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
const EGL_CONTEXT_OPENGL_PROFILE_MASK: i32 = 0x3093;
const EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY: i32 = 0x3094;
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_SUCCESS: u32 = 0x3000;
const EGL_VERSION: i32 = 0x3053;
const EGL_EXTENSIONS: i32 = 0x3055;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetDisplay(display_id: *mut std::ffi::c_void) -> *mut std::ffi::c_void {
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
        EGL_VERSION => {
            static VERSION: &[u8] = b"1.4 FluorateGL\0";
            VERSION.as_ptr() as *const c_char
        }
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
    if attrib_list.is_null() {
        return backend::with_egl_dispatch(|d| unsafe {
            (d.create_context)(dpy, config, share_context, std::ptr::null())
        });
    }

    let mut new_attribs = Vec::new();
    let mut i = 0;

    loop {
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
    log::info!(
        "[EGL] eglMakeCurrent dpy={:?} draw={:?} read={:?} ctx={:?}",
        dpy,
        draw,
        read,
        ctx
    );
    let result = backend::with_egl_dispatch(|d| unsafe { (d.make_current)(dpy, draw, read, ctx) });
    log::info!("[EGL] eglMakeCurrent result=0x{:04X}", result);
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
        unsafe { *value = crate::config::REPORTED_GL_MAJOR };
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
    if is_key {
        log::debug!("[FluorateGL] eglGetProcAddress({}) -> EGL driver", name);
    }
    backend::with_egl_dispatch(|d| unsafe { (d.get_proc_address)(proc_name) })
}
