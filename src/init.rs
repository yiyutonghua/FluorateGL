//! FluorateGL 初始化模块
//!
//! 负责：
//! - 通过 `#[ctor]` 在库加载时自动初始化日志、后端、EGL/GLES 加载
//! - 捕获自身库句柄（用于 `eglGetProcAddress` 返回本库的函数指针）
//! - 提供 `fluorategl_init` 显式初始化入口（供宿主或测试调用）

use crate::config::Config;
use crate::util;
use ctor::ctor;
use std::sync::OnceLock;

/// FluorateGL 版本号
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 我们自己库的 dlopen 句柄，用于 eglGetProcAddress 中确保返回我们的函数指针。
/// 使用 usize 存储（指针本身是 Send + Sync 的，只是 Rust 不自动为裸指针实现）。
static SELF_HANDLE: OnceLock<usize> = OnceLock::new();

#[ctor(unsafe)]
fn auto_init() {
    let ret = fluorategl_init();
    if ret != 0 {
        eprintln!("FluorateGL auto-init failed: {}", ret);
    }
}

/// 捕获自身库句柄（通过 dladdr 定位本库路径再 dlopen）。
///
/// 用于 `eglGetProcAddress` 中 `dlsym(self_handle, ...)`，确保返回的是
/// FluorateGL 拦截层（而非底层 GLES 驱动）的函数指针。
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

/// FluorateGL 显式初始化入口
///
/// 在 `#[ctor]` 自动调用之外，也允许宿主或测试显式调用。
/// 流程：加载配置 → 初始化日志 → 捕获库句柄 → 加载 EGL/GLES 后端。
///
/// 返回 0 表示成功，非 0 表示失败（仅 `eprintln` 路径会用，目前始终返回 0）。
#[unsafe(no_mangle)]
pub extern "C" fn fluorategl_init() -> i32 {
    let cfg = Config::from_env();
    util::log::init(&cfg);

    log::info!("[FluorateGL] v{} Initializing...", VERSION);
    log::info!(
        "[FluorateGL] Backend: {:?}, LogLevel: {:?}",
        cfg.backend,
        cfg.log_level
    );

    // 在初始化日志后立即捕获自己的库句柄（用于 eglGetProcAddress）
    capture_self_handle();

    crate::backend::set_config(cfg);

    if cfg.skip_backend {
        log::info!("[FluorateGL] FLUORATEGL_SKIP_BACKEND=1, skip EGL/GLES loading");
    } else {
        // 注意：EGL/GLES 加载失败时不返回错误，只发出警告。
        // 翻译管线（GLSL→SPIR-V→GLSL ES）是纯 CPU 操作，不依赖 EGL/GLES，
        // 因此即使在无 GLES 后端的环境（如纯翻译测试）也能正常工作。
        // MC 运行时若 GLES 缺失，GL 调用会走 stub dispatch（已有机制）。
        if crate::backend::init_egl().is_err() {
            log::warn!("[FluorateGL] EGL library unavailable (translation pipeline still works)");
        } else {
            log::info!("[FluorateGL] EGL library loaded");
        }

        if crate::backend::init_gles().is_err() {
            log::warn!("[FluorateGL] GLES library unavailable (translation pipeline still works)");
        } else {
            log::info!("[FluorateGL] GLES library loaded");
        }
    }

    log::info!("[FluorateGL] v{} Initialized successfully", VERSION);
    crate::backend::mark_initialized();
    0
}
