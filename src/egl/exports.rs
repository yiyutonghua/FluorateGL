use super::dispatcher;
use std::ffi::c_void;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglInitialize(dpy: *mut std::ffi::c_void, major: *mut i32, minor: *mut i32) -> u32 {
    dispatcher::initialize(dpy, major, minor)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglBindAPI(_api: u32) -> u32 {
    dispatcher::bind_api(0x30A0) // EGL_OPENGL_ES_API
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

    // 修改 attrib_list：OpenGL 3.2 Core → GLES 3.0
    let mut new_attribs = Vec::new();
    let mut i = 0;
    
    loop {
        let attr = unsafe { *attrib_list.offset(i) };
        
        if attr == 0x3038 { // EGL_NONE
            break;
        }
        
        let value = unsafe { *attrib_list.offset(i + 1) };
        
        match attr {
            0x3098 => { // EGL_CONTEXT_CLIENT_VERSION
                new_attribs.push(0x3098);
                new_attribs.push(3); // 强制 GLES 3.0
            }
            0x3093 => { // EGL_CONTEXT_OPENGL_PROFILE_MASK - 跳过
                // 不添加
            }
            0x3094 => { // EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY - 跳过
                // GLES 不支持
            }
            _ => {
                new_attribs.push(attr);
                new_attribs.push(value);
            }
        }
        
        i += 2;
    }
    
    new_attribs.push(0x3038); // EGL_NONE
    
    let ptr = new_attribs.as_ptr();
    std::mem::forget(new_attribs); // 防止释放，因为 C 端会长期持有
    
    dispatcher::create_context(dpy, config, share_context, ptr)
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
pub extern "C" fn eglSwapBuffers(dpy: *mut std::ffi::c_void, surface: *mut std::ffi::c_void) -> u32 {
    dispatcher::swap_buffers(dpy, surface)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglTerminate(dpy: *mut c_void) -> u32 {
    dispatcher::terminate(dpy)
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
pub extern "C" fn eglDestroySurface(dpy: *mut c_void, surface: *mut c_void) -> u32 {
    dispatcher::destroy_surface(dpy, surface)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglDestroyContext(dpy: *mut c_void, ctx: *mut c_void) -> u32 {
    dispatcher::destroy_context(dpy, ctx)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglQueryContext(
    dpy: *mut c_void,
    ctx: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    if attribute == 0x3098 { // EGL_CONTEXT_CLIENT_VERSION
        unsafe { *value = 3 }; // 返回 3.0
        return 0x3000; // EGL_SUCCESS
    }
    dispatcher::query_context(dpy, ctx, attribute, value)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetError() -> u32 {
    dispatcher::get_error()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn eglGetProcAddress(proc_name: *const i8) -> *mut c_void {
    dispatcher::get_proc_address(proc_name)
}
