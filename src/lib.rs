mod backend;
mod config;
mod egl;
mod egl_sys;
mod gl;
mod state;



use config::Config;
use libc::c_char;

#[unsafe(no_mangle)]
pub extern "C" fn fluorategl_init() -> i32 {
   
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug),
    );

    let cfg = Config::from_env();

    log::info!("[FluorateGL] Initializing...");
    log::info!("[FluorateGL] Backend: {:?}", cfg.backend);
    
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
            let vendor = (dispatch.get_string)(0x1F00);   // GL_VENDOR

            log::info!("[FluorateGL] GLES version: {}", c_str_to_string(version));
            log::info!("[FluorateGL] GPU: {}", c_str_to_string(renderer));
            log::info!("[FluorateGL] Vendor: {}", c_str_to_string(vendor));
        }
    }
    
    log::info!("[FluorateGL] Initialized successfully");
    0
}
/*
#[used]
#[unsafe(link_section = ".init_array")]
static INIT: extern "C" fn() = init_wrapper;

extern "C" fn init_wrapper() {
    let _ = fluorategl_init();
}
*/
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
