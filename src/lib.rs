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
use std::sync::OnceLock;

/// 我们自己库的 dlopen 句柄，用于 eglGetProcAddress 中确保返回我们的函数指针
/// 使用 usize 存储（指针本身是 Send + Sync 的，只是 Rust 不自动为裸指针实现）
static SELF_HANDLE: OnceLock<usize> = OnceLock::new();

fn capture_self_handle() {
    // 使用 dladdr 获取 fluorategl_init 所在库的路径，然后 dlopen 获取句柄
    let addr = fluorategl_init as *const ();
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    if unsafe { libc::dladdr(addr as *const _, &mut info) } != 0 {
        if !info.dli_fname.is_null() {
            // 先尝试 RTLD_NOLOAD（不重新加载，只增加引用计数）
            let handle =
                unsafe { libc::dlopen(info.dli_fname, libc::RTLD_NOW | libc::RTLD_NOLOAD) };
            // Android 旧版本可能不支持 RTLD_NOLOAD，回退到普通 dlopen
            let handle = if handle.is_null() {
                unsafe { libc::dlopen(info.dli_fname, libc::RTLD_NOW) }
            } else {
                handle
            };
            if !handle.is_null() {
                let _ = SELF_HANDLE.set(handle as usize);
                log::info!(
                    "[FluorateGL] Captured self handle {:?} from {:?}",
                    handle,
                    unsafe { std::ffi::CStr::from_ptr(info.dli_fname) }
                );
            } else {
                log::warn!("[FluorateGL] dlopen failed for self handle: {:?}", unsafe {
                    std::ffi::CStr::from_ptr(info.dli_fname)
                });
            }
        }
    } else {
        log::warn!(
            "[FluorateGL] dladdr failed, eglGetProcAddress may return wrong function pointers"
        );
    }
}

/// 获取我们自己库的句柄，用于 dlsym 查找
pub fn get_self_handle() -> Option<*mut libc::c_void> {
    SELF_HANDLE.get().map(|h| *h as *mut libc::c_void)
}

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
    log::info!(
        "[FluorateGL] Backend: {:?}, LogLevel: {:?}",
        cfg.backend,
        cfg.log_level
    );

    // 在初始化日志后立即捕获自己的库句柄（用于 eglGetProcAddress）
    capture_self_handle();

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
