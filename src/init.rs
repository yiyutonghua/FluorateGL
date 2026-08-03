//! FluorateGL 初始化模块
//!
//! 负责：
//! - 通过 `#[ctor]` 在库加载时只做配置解析与日志初始化（后端 EGL/GLES 加载
//!   惰性化到首次 GL/EGL 调用，见 `ensure_backend_initialized`）
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
    // P0-A：ctor 只做配置解析与日志初始化，不再 dlopen 后端（后端惰性化见 ensure_backend_initialized）
    let cfg = Config::from_env();
    util::log::init(&cfg);
    crate::backend::set_config(cfg);
}

/// 后端惰性初始化锁：首次 GL/EGL 调用时执行一次 dlopen 初始化。
/// 失败后永久保持失败值（Err），后续调用不再重试，走 stub 兜底。
static BACKEND_INIT: OnceLock<Result<(), &'static str>> = OnceLock::new();

/// 首次 GL/EGL 调用时惰性初始化后端（dlopen EGL/GLES）。
/// OnceLock 保证并发首调用只执行一次；失败后永久保持失败值，后续调用走 stub 兜底。
pub fn ensure_backend_initialized() {
    let _ = BACKEND_INIT.get_or_init(|| {
        capture_self_handle(); // P0-C 已删回退，无重入风险；直接调用（严禁调用 get_self_handle——闭包内重入 get_or_init 会 panic）
        let cfg = crate::backend::config(); // 见 backend/mod.rs 的 config() 访问器
        if cfg.skip_backend {
            log::info!("[FluorateGL] FLUORATEGL_SKIP_BACKEND=1, skip EGL/GLES loading");
            return Ok(());
        }
        let mut failed: Option<&'static str> = None;
        if let Err(e) = crate::backend::init_egl() {
            log::error!(
                "[FluorateGL] EGL library unavailable ({}): {}",
                cfg.egl_lib_name(),
                e
            );
            if cfg.fail_fast {
                log::error!("[FluorateGL] FAIL_FAST: EGL 加载失败，abort");
                std::process::abort();
            }
            failed = Some(e);
        }
        if let Err(e) = crate::backend::init_gles() {
            log::error!(
                "[FluorateGL] GLES library unavailable ({}): {}",
                cfg.gles_lib_name(),
                e
            );
            if cfg.fail_fast {
                log::error!("[FluorateGL] FAIL_FAST: GLES 加载失败，abort");
                std::process::abort();
            }
            failed = Some(e);
        }
        crate::backend::mark_initialized();
        match failed {
            Some(e) => Err(e),
            None => Ok(()),
        }
    });
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
            // 仅尝试 RTLD_NOLOAD（不重新加载，只增加引用计数）。
            // 旧平台不支持 RTLD_NOLOAD 时不做二次 dlopen 回退（避免 ctor 重入）；
            // P3-A 已实现：SYMBOLS 表过滤后再 dlsym(self_handle)（见 egl/exports.rs）。
            let handle =
                unsafe { libc::dlopen(info.dli_fname, libc::RTLD_NOW | libc::RTLD_NOLOAD) };
            if !handle.is_null() {
                let _ = SELF_HANDLE.set(handle as usize);
                log::info!(
                    "[FluorateGL] Captured self handle {:?} from {:?}",
                    handle,
                    unsafe { std::ffi::CStr::from_ptr(info.dli_fname) }
                );
            } else {
                log::warn!(
                    "[FluorateGL] RTLD_NOLOAD dlopen failed for self handle \
                     (eglGetProcAddress 将走 RTLD_DEFAULT 兜底)"
                );
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
    ensure_backend_initialized(); // 保证 SELF_HANDLE 已尝试捕获
    SELF_HANDLE.get().map(|h| *h as *mut libc::c_void)
}

/// FluorateGL 显式初始化入口
///
/// 在 `#[ctor]` 自动调用之外，也允许宿主或测试显式调用。
/// P0-A 重构后为薄包装：完整初始化（捕获库句柄 + dlopen EGL/GLES 后端）
/// 惰性化到 `ensure_backend_initialized`，本函数只负责触发一次。
///
/// 返回 0 表示成功（后端加载失败不在此处报错，仅降级为 stub，见惰性初始化）。
#[unsafe(no_mangle)]
pub extern "C" fn fluorategl_init() -> i32 {
    ensure_backend_initialized();
    0
}
