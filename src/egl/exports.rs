use super::dispatcher;
use libc::c_char;
use std::ffi::c_void;

// EGL enum constants
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
    dispatcher::get_display(display_id)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglInitialize(dpy: *mut std::ffi::c_void, major: *mut i32, minor: *mut i32) -> u32 {
    let result = dispatcher::initialize(dpy, major, minor);
    // Ensure the application sees EGL 1.4 even if the driver reports lower.
    if result == EGL_SUCCESS {
        if !major.is_null() {
            unsafe { *major = 1 };
        }
        if !minor.is_null() {
            unsafe { *minor = 4 };
        }
    }
    result
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglTerminate(dpy: *mut c_void) -> u32 {
    dispatcher::terminate(dpy)
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
        _ => dispatcher::query_string(dpy, name),
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
    dispatcher::get_configs(dpy, configs, config_size, num_config)
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
    dispatcher::choose_config(dpy, attrib_list, configs, config_size, num_config)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetConfigAttrib(
    dpy: *mut c_void,
    config: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    dispatcher::get_config_attrib(dpy, config, attribute, value)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCreateWindowSurface(
    dpy: *mut c_void,
    config: *mut c_void,
    win: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    dispatcher::create_window_surface(dpy, config, win, attrib_list)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCreatePbufferSurface(
    dpy: *mut c_void,
    config: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    dispatcher::create_pbuffer_surface(dpy, config, attrib_list)
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
    dispatcher::create_pbuffer_from_client_buffer(dpy, buftype, buffer, config, attrib_list)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCreatePixmapSurface(
    dpy: *mut c_void,
    config: *mut c_void,
    pixmap: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    dispatcher::create_pixmap_surface(dpy, config, pixmap, attrib_list)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglDestroySurface(dpy: *mut c_void, surface: *mut c_void) -> u32 {
    dispatcher::destroy_surface(dpy, surface)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglSurfaceAttrib(
    dpy: *mut c_void,
    surface: *mut c_void,
    attribute: i32,
    value: i32,
) -> u32 {
    dispatcher::surface_attrib(dpy, surface, attribute, value)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglBindTexImage(dpy: *mut c_void, surface: *mut c_void, buffer: i32) -> u32 {
    dispatcher::bind_tex_image(dpy, surface, buffer)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglReleaseTexImage(dpy: *mut c_void, surface: *mut c_void, buffer: i32) -> u32 {
    dispatcher::release_tex_image(dpy, surface, buffer)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglBindAPI(_api: u32) -> u32 {
    dispatcher::bind_api(EGL_OPENGL_ES_API)
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
        return dispatcher::create_context(dpy, config, share_context, std::ptr::null());
    }

    // Rewrite attrib_list: OpenGL 3.2 Core -> GLES 3.2
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
                new_attribs.push(3); // Force GLES 3.x
            }
            EGL_CONTEXT_OPENGL_PROFILE_MASK => {
                // GLES does not have core/compatibility profiles.
            }
            EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY => {
                // GLES supports this via EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT only.
                // Skip for safety.
            }
            _ => {
                new_attribs.push(attr);
                new_attribs.push(value);
            }
        }

        i += 2;
    }

    new_attribs.push(EGL_NONE);

    // The underlying EGL implementation reads attrib_list synchronously and does
    // not retain the pointer, so it is safe to keep the Vec scoped to this call.
    let ptr = new_attribs.as_ptr();

    dispatcher::create_context(dpy, config, share_context, ptr)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglDestroyContext(dpy: *mut c_void, ctx: *mut c_void) -> u32 {
    dispatcher::destroy_context(dpy, ctx)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglMakeCurrent(
    dpy: *mut std::ffi::c_void,
    draw: *mut std::ffi::c_void,
    read: *mut std::ffi::c_void,
    ctx: *mut std::ffi::c_void,
) -> u32 {
    dispatcher::make_current(dpy, draw, read, ctx)
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
        unsafe { *value = 3 }; // Report GLES 3.x
        return EGL_SUCCESS;
    }
    dispatcher::query_context(dpy, ctx, attribute, value)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglQuerySurface(
    dpy: *mut c_void,
    surface: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    dispatcher::query_surface(dpy, surface, attribute, value)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetCurrentContext() -> *mut c_void {
    dispatcher::get_current_context()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetCurrentSurface(readdraw: i32) -> *mut c_void {
    dispatcher::get_current_surface(readdraw)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetCurrentDisplay() -> *mut c_void {
    dispatcher::get_current_display()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglWaitClient() -> u32 {
    dispatcher::wait_client()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglWaitNative(engine: i32) -> u32 {
    dispatcher::wait_native(engine)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglWaitGL() -> u32 {
    dispatcher::wait_gl()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglReleaseThread() -> u32 {
    dispatcher::release_thread()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglSwapBuffers(dpy: *mut std::ffi::c_void, surface: *mut std::ffi::c_void) -> u32 {
    dispatcher::swap_buffers(dpy, surface)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglSwapInterval(dpy: *mut c_void, interval: i32) -> u32 {
    dispatcher::swap_interval(dpy, interval)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglCopyBuffers(dpy: *mut c_void, surface: *mut c_void, target: *mut c_void) -> u32 {
    dispatcher::copy_buffers(dpy, surface, target)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetError() -> u32 {
    dispatcher::get_error()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetProcAddress(proc_name: *const c_char) -> *mut c_void {
    if proc_name.is_null() {
        return std::ptr::null_mut();
    }

    // First, try to resolve the symbol from FluorateGL itself.
    // This lets applications retrieve our wrapped OpenGL / EGL entry points.
    unsafe {
        let local = libc::dlsym(std::ptr::null_mut(), proc_name as *const libc::c_char);
        if !local.is_null() {
            return local;
        }
    }

    // Fallback to the underlying EGL implementation.
    dispatcher::get_proc_address(proc_name)
}
