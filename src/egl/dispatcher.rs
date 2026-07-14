use crate::backend;
use libc::c_char;
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

pub fn terminate(dpy: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.terminate)(dpy)
    })
}

pub fn query_string(dpy: *mut c_void, name: i32) -> *const c_char {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.query_string)(dpy, name)
    })
}

pub fn get_configs(
    dpy: *mut c_void,
    configs: *mut c_void,
    config_size: i32,
    num_config: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_configs)(dpy, configs, config_size, num_config)
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

pub fn create_pbuffer_surface(
    dpy: *mut c_void,
    config: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.create_pbuffer_surface)(dpy, config, attrib_list)
    })
}

pub fn create_pbuffer_from_client_buffer(
    dpy: *mut c_void,
    buftype: u32,
    buffer: *mut c_void,
    config: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.create_pbuffer_from_client_buffer)(dpy, buftype, buffer, config, attrib_list)
    })
}

pub fn create_pixmap_surface(
    dpy: *mut c_void,
    config: *mut c_void,
    pixmap: *mut c_void,
    attrib_list: *const i32,
) -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.create_pixmap_surface)(dpy, config, pixmap, attrib_list)
    })
}

pub fn destroy_surface(dpy: *mut c_void, surface: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.destroy_surface)(dpy, surface)
    })
}

pub fn surface_attrib(
    dpy: *mut c_void,
    surface: *mut c_void,
    attribute: i32,
    value: i32,
) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.surface_attrib)(dpy, surface, attribute, value)
    })
}

pub fn bind_tex_image(dpy: *mut c_void, surface: *mut c_void, buffer: i32) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.bind_tex_image)(dpy, surface, buffer)
    })
}

pub fn release_tex_image(dpy: *mut c_void, surface: *mut c_void, buffer: i32) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.release_tex_image)(dpy, surface, buffer)
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

pub fn destroy_context(dpy: *mut c_void, ctx: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.destroy_context)(dpy, ctx)
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

pub fn query_surface(
    dpy: *mut c_void,
    surface: *mut c_void,
    attribute: i32,
    value: *mut i32,
) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.query_surface)(dpy, surface, attribute, value)
    })
}

pub fn get_current_context() -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_current_context)()
    })
}

pub fn get_current_surface(readdraw: i32) -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_current_surface)(readdraw)
    })
}

pub fn get_current_display() -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_current_display)()
    })
}

pub fn wait_client() -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.wait_client)()
    })
}

pub fn wait_native(engine: i32) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.wait_native)(engine)
    })
}

pub fn wait_gl() -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.wait_gl)()
    })
}

pub fn release_thread() -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.release_thread)()
    })
}

pub fn bind_api(api: u32) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.bind_api)(api)
    })
}

#[allow(dead_code)]
pub fn query_api() -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.query_api)()
    })
}

pub fn swap_buffers(dpy: *mut std::ffi::c_void, surface: *mut std::ffi::c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.swap_buffers)(dpy, surface)
    })
}

pub fn swap_interval(dpy: *mut c_void, interval: i32) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.swap_interval)(dpy, interval)
    })
}

pub fn copy_buffers(dpy: *mut c_void, surface: *mut c_void, target: *mut c_void) -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.copy_buffers)(dpy, surface, target)
    })
}

pub fn get_error() -> u32 {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_error)()
    })
}

pub fn get_proc_address(proc_name: *const c_char) -> *mut c_void {
    backend::with_egl_dispatch(|dispatch| unsafe {
        (dispatch.get_proc_address)(proc_name)
    })
}
