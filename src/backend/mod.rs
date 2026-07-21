pub mod dispatch;
pub mod loader;

use crate::config::Config;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

// 全局状态
static CONFIG: OnceLock<Config> = OnceLock::new();
pub static GLES_DISPATCH: OnceLock<dispatch::GlesDispatch> = OnceLock::new();
static EGL_DISPATCH: OnceLock<crate::egl_sys::dispatch::EglDispatch> = OnceLock::new();
static INIT_ONCE: OnceLock<()> = OnceLock::new();

/*pub fn ensure_initialized() {
    INIT_ONCE.get_or_init(|| {
        let _ = crate::fluorategl_init();
    });
}*/

pub fn mark_initialized() {
    let _ = INIT_ONCE.set(());
}

pub fn set_config(config: Config) {
    let _ = CONFIG.set(config);
}

pub fn init_gles() -> Result<(), &'static str> {
    let config = CONFIG.get().expect("config not set");

    let loader = loader::GlesLoader::new(config)?;
    let dispatch = dispatch::GlesDispatch::load_from(&loader)
        .ok_or("failed to load required GLES function")?;

    let _ = GLES_DISPATCH.set(dispatch);

    Box::leak(Box::new(loader));
    Ok(())
}

pub fn init_egl() -> Result<(), &'static str> {
    let config = CONFIG.get().expect("config not set");

    let loader = crate::egl_sys::loader::EglLoader::new(config)?;
    let dispatch = crate::egl_sys::dispatch::EglDispatch::load_from(&loader)
        .ok_or("failed to load required EGL function")?;

    let _ = EGL_DISPATCH.set(dispatch);

    Box::leak(Box::new(loader));
    Ok(())
}

pub fn with_gles_dispatch<F, R>(f: F) -> R
where
    F: FnOnce(&dispatch::GlesDispatch) -> R,
{
    static FIRST_CALL: AtomicBool = AtomicBool::new(true);
    if FIRST_CALL.swap(false, Ordering::Relaxed) {
        log::info!("[FluorateGL] === 首次 GL 调用，游戏渲染管线已启动 ===");
        suppress_debug_noise();
        log_gpu_info();
    }

    //ensure_initialized();
    let dispatch = GLES_DISPATCH.get().unwrap_or_else(|| {
        static STUB: OnceLock<dispatch::GlesDispatch> = OnceLock::new();
        STUB.get_or_init(dispatch::GlesDispatch::all_stub)
    });
    f(dispatch)
}

/// 在首次 GL 调用时（EGL 上下文已创建）查询并记录 GPU 信息。
/// 不能在 `fluorategl_init()` 中查询，因为那时 EGL 上下文尚未创建。
fn log_gpu_info() {
    use libc::c_char;
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

    if let Some(dispatch) = GLES_DISPATCH.get() {
        unsafe {
            let version = (dispatch.get_string)(0x1F02); // GL_VERSION
            let renderer = (dispatch.get_string)(0x1F01); // GL_RENDERER
            let vendor = (dispatch.get_string)(0x1F00); // GL_VENDOR

            log::info!("[FluorateGL] GLES version: {}", c_str_to_string(version));
            log::info!("[FluorateGL] GPU: {}", c_str_to_string(renderer));
            log::info!("[FluorateGL] Vendor: {}", c_str_to_string(vendor));
        }
    }
}

/// 屏蔽 GLES 驱动的 PERFORMANCE / OTHER 类型 Debug 消息，
/// 避免 "Packing allocations" / "high level of unsubmitted work" 等刷屏。
/// 只保留 ERROR 类型消息用于诊断。
fn suppress_debug_noise() {
    const GL_DONT_CARE: u32 = 0x1100;
    const GL_DEBUG_TYPE_PERFORMANCE: u32 = 0x8250;
    const GL_DEBUG_TYPE_OTHER: u32 = 0x8251;
    const GL_FALSE: u8 = 0;

    let dispatch = GLES_DISPATCH.get().unwrap_or_else(|| {
        static STUB: OnceLock<dispatch::GlesDispatch> = OnceLock::new();
        STUB.get_or_init(dispatch::GlesDispatch::all_stub)
    });

    // 如果 debug_message_control 是 stub（GLES 驱动不支持），则静默跳过
    if dispatch.debug_message_control as *const () == dispatch.stub as *const () {
        return;
    }

    unsafe {
        (dispatch.debug_message_control)(
            GL_DONT_CARE,
            GL_DEBUG_TYPE_PERFORMANCE,
            GL_DONT_CARE,
            0,
            std::ptr::null(),
            GL_FALSE,
        );
        (dispatch.debug_message_control)(
            GL_DONT_CARE,
            GL_DEBUG_TYPE_OTHER,
            GL_DONT_CARE,
            0,
            std::ptr::null(),
            GL_FALSE,
        );
    }

    log::info!("[FluorateGL] 已屏蔽 GLES Debug PERFORMANCE/OTHER 消息");
}

pub fn with_egl_dispatch<F, R>(f: F) -> R
where
    F: FnOnce(&crate::egl_sys::dispatch::EglDispatch) -> R,
{
    static FIRST_EGL_CALL: AtomicBool = AtomicBool::new(true);
    if FIRST_EGL_CALL.swap(false, Ordering::Relaxed) {
        log::info!("[FluorateGL] === 首次 EGL 调用 ===");
    }

    //ensure_initialized();
    let dispatch = EGL_DISPATCH.get().unwrap_or_else(|| {
        static STUB: OnceLock<crate::egl_sys::dispatch::EglDispatch> = OnceLock::new();
        STUB.get_or_init(crate::egl_sys::dispatch::EglDispatch::all_stub)
    });
    f(dispatch)
}
