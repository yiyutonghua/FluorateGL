mod backend;
mod config;
mod egl;
mod egl_sys;
mod gl;
pub mod shader_translator;
mod state;
mod util;

use config::Config;
use ctor::ctor;
use libc::c_char;

#[ctor(unsafe)]
fn auto_init() {
    let ret = fluorategl_init();
    if ret != 0 {
        eprintln!("FluorateGL auto-init failed: {}", ret);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn fluorategl_init() -> i32 {
    let cfg = Config::from_env();
    util::log::init(&cfg);

    log::info!("[FluorateGL] Initializing...");
    log::info!("[FluorateGL] Backend: {:?}, LogLevel: {:?}", cfg.backend, cfg.log_level);

    backend::set_config(cfg);

    // 加载Gles
    if backend::init_gles().is_err() {
        log::error!("[FluorateGL] Failed to initialize GLES");
        return -1;
    }
    log::info!("[FluorateGL] GLES library loaded");

    // 加载EGL
    if backend::init_egl().is_err() {
        log::error!("[FluorateGL] Failed to initialize EGL");
        return -2;
    }
    log::info!("[FluorateGL] EGL library loaded");

    // 查询并记录 GPU 信息
    // Use the dispatch table directly to avoid `with_gles_dispatch` re-entering
    // `fluorategl_init` through `ensure_initialized`.
    if let Some(dispatch) = backend::GLES_DISPATCH.get() {
        unsafe {
            let version = (dispatch.get_string)(0x1F02); // GL_VERSION
            let renderer = (dispatch.get_string)(0x1F01); // GL_RENDERER
            let vendor = (dispatch.get_string)(0x1F00); // GL_VENDOR

            log::info!("[FluorateGL] GLES version: {}", c_str_to_string(version));
            log::info!("[FluorateGL] GPU: {}", c_str_to_string(renderer));
            log::info!("[FluorateGL] Vendor: {}", c_str_to_string(vendor));
        }
    }

    log::info!("[FluorateGL] Initialized successfully");
    crate::backend::mark_initialized();
    0
}

unsafe fn c_str_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return "(null)".to_string();
    }
    unsafe {
        std::ffi::CStr::from_ptr(ptr as *const _)
            .to_string_lossy()
            .into_owned()
    }
}
