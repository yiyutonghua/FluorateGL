use crate::backend;
use std::ffi::c_void;

pub fn get_display(display_id: *mut std::ffi::c_void) -> *mut std::ffi::c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_display)(display_id)
    })
}

pub fn initialize(dpy: *mut std::ffi::c_void, major: *mut i32, minor: *mut i32) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.initialize)(dpy, major, minor)
    })
}

pub fn bind_api(api: u32) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.bind_api)(api)
    })
}

pub fn create_context(
    dpy: *mut std::ffi::c_void,
    config: *mut std::ffi::c_void,
    share_context: *mut std::ffi::c_void,
    attrib_list: *const i32,
) -> *mut std::ffi::c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.create_context)(dpy, config, share_context, attrib_list)
    })
}

pub fn make_current(
    dpy: *mut std::ffi::c_void,
    draw: *mut std::ffi::c_void,
    read: *mut std::ffi::c_void,
    ctx: *mut std::ffi::c_void,
) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.make_current)(dpy, draw, read, ctx)
    })
}

pub fn swap_buffers(dpy: *mut std::ffi::c_void, surface: *mut std::ffi::c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.swap_buffers)(dpy, surface)
    })
}

pub fn terminate(dpy: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.terminate)(dpy)
    })
}

pub fn choose_config(
    dpy: *mut c_void,
    attrib_list: *const i32,
    configs: *mut c_void,
    config_size: i32,
    num_config: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.choose_config)(dpy, attrib_list, configs, config_size, num_config)
    })
}

pub fn get_config_attrib(
    dpy: *mut c_void,
    config: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_config_attrib)(dpy, config, attribute, value)
    })
}

pub fn create_window_surface(
    dpy: *mut c_void,
    config: *mut c_void,
    win: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.create_window_surface)(dpy, config, win, attrib_list)
    })
}

pub fn destroy_surface(dpy: *mut c_void, surface: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.destroy_surface)(dpy, surface)
    })
}

pub fn destroy_context(dpy: *mut c_void, ctx: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.destroy_context)(dpy, ctx)
    })
}

pub fn query_context(
    dpy: *mut c_void,
    ctx: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.query_context)(dpy, ctx, attribute, value)
    })
}

pub fn get_error() -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_error)()
    })
}

pub fn get_proc_address(proc_name: *const i8) -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_proc_address)(proc_name)
    })
}
