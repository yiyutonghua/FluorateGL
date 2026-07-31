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
use std::sync::OnceLock;

/// FluorateGL 版本号
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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

    log::info!("[FluorateGL] v{} Initializing...", VERSION);
    log::info!(
        "[FluorateGL] Backend: {:?}, LogLevel: {:?}",
        cfg.backend,
        cfg.log_level
    );

    // 在初始化日志后立即捕获自己的库句柄（用于 eglGetProcAddress）
    capture_self_handle();

    backend::set_config(cfg);

    // FLUORATEGL_SKIP_BACKEND=1 时跳过 EGL/GLES 库加载。
    // 用于 fork worker 等只需翻译管线（纯 CPU）的场景，避免重复 dlopen/dlsym 开销。
    // MC 运行时不设置此变量，正常加载 EGL/GLES。
    let skip_backend = std::env::var("FLUORATEGL_SKIP_BACKEND")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if skip_backend {
        log::info!("[FluorateGL] FLUORATEGL_SKIP_BACKEND=1, skip EGL/GLES loading");
    } else {
        // 加载EGL
        // 注意：EGL/GLES 加载失败时不返回错误，只发出警告。
        // 翻译管线（GLSL→SPIR-V→GLSL ES）是纯 CPU 操作，不依赖 EGL/GLES，
        // 因此即使在无 GLES 后端的环境（如纯翻译测试）也能正常工作。
        // MC 运行时若 GLES 缺失，GL 调用会走 stub dispatch（已有机制）。
        if backend::init_egl().is_err() {
            log::warn!("[FluorateGL] EGL library unavailable (translation pipeline still works)");
        } else {
            log::info!("[FluorateGL] EGL library loaded");
        }

        // 加载Gles
        if backend::init_gles().is_err() {
            log::warn!("[FluorateGL] GLES library unavailable (translation pipeline still works)");
        } else {
            log::info!("[FluorateGL] GLES library loaded");
        }
    }

    log::info!("[FluorateGL] v{} Initialized successfully", VERSION);
    crate::backend::mark_initialized();
    0
}

// ── 离线编译测试支持（供 glslang test suite 使用） ──────────────────────────
//
// 以下公开函数用于在不加载完整 MC 的环境下，验证翻译后的 GLSL ES 能否在
// 真实 GLES 驱动上编译。仅在 EGL/GLES 后端可用时工作；否则返回错误。

use std::ffi::CString;

/// 缓存的 surfaceless GLES 上下文就绪标志
static GLES_CONTEXT_READY: OnceLock<bool> = OnceLock::new();

const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_NONE: i32 = 0x3038;
const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
const EGL_SURFACE_TYPE: i32 = 0x3033;
const EGL_RENDERABLE_TYPE: i32 = 0x304C;
const EGL_PBUFFER_BIT: i32 = 0x0001;
const EGL_OPENGL_ES2_BIT: i32 = 0x0004;
const EGL_OPENGL_ES3_BIT: i32 = 0x0040;
const EGL_NO_SURFACE: *mut std::ffi::c_void = std::ptr::null_mut();

/// 尝试创建 surfaceless GLES 3 上下文用于离线编译测试。
///
/// 重复调用安全（只创建一次）。成功返回 true，失败返回 false。
/// 需要 `fluorategl_init()` 已调用，且 EGL/GLES 后端可用。
///
/// 适配策略（保证 Android / ANGLE / Linux surfaceless 三平台兼容）：
///   1. eglGetDisplay(EGL_DEFAULT_DISPLAY=NULL)
///      - Android: 返回默认 display
///      - Linux surfaceless: 配合 EGL_PLATFORM=surfaceless 返回软件 display
///      - ANGLE: 返回 ANGLE display
///   2. eglChooseConfig 依次尝试 GLES3 → GLES2 → 无过滤，适配各平台 config 能力
///   3. eglCreateContext 指定 client version 3（GLES 3.0+，兼容 GLSL ES 300/310/320）
pub fn ensure_gles_context() -> bool {
    *GLES_CONTEXT_READY.get_or_init(|| unsafe {
        let egl = match crate::backend::egl_dispatch() {
            Some(d) => d,
            None => {
                log::warn!("[glslang] EGL dispatch unavailable, skip GLES compile test");
                return false;
            }
        };

        // 清空 EGL 错误
        let _ = (egl.get_error)();

        let dpy = (egl.get_display)(std::ptr::null_mut());
        if dpy.is_null() {
            log::warn!(
                "[glslang] eglGetDisplay returned NULL, err=0x{:x}",
                (egl.get_error)()
            );
            return false;
        }

        let mut maj = 0i32;
        let mut min = 0i32;
        // eglInitialize 返回 EGLBoolean（EGL_TRUE=1/EGL_FALSE=0），不是 EGL_SUCCESS(0x3000)
        if (egl.initialize)(dpy, &mut maj, &mut min) == 0 {
            log::warn!(
                "[glslang] eglInitialize failed, err=0x{:x}",
                (egl.get_error)()
            );
            return false;
        }
        log::info!("[glslang] EGL {}.{} initialized", maj, min);

        // 选择 config：依次尝试 GLES3 → GLES2 → 无 renderable 过滤
        // 不同平台/驱动对 EGL_OPENGL_ES3_BIT 的支持不一致：
        //   - Mesa surfaceless: 支持 GLES3_BIT
        //   - 某些旧 Android: 只有 GLES2_BIT 但仍可创建 GLES3 context
        //   - ANGLE: 通常两者都支持
        let mut cfg = std::ptr::null_mut();
        let mut nc: i32;
        let mut chosen = false;

        let filters: &[(&str, &[i32])] = &[
            (
                "PBUFFER+GLES3",
                &[
                    EGL_SURFACE_TYPE,
                    EGL_PBUFFER_BIT,
                    EGL_RENDERABLE_TYPE,
                    EGL_OPENGL_ES3_BIT,
                    EGL_NONE,
                ],
            ),
            (
                "PBUFFER+GLES2",
                &[
                    EGL_SURFACE_TYPE,
                    EGL_PBUFFER_BIT,
                    EGL_RENDERABLE_TYPE,
                    EGL_OPENGL_ES2_BIT,
                    EGL_NONE,
                ],
            ),
            (
                "PBUFFER-only",
                &[EGL_SURFACE_TYPE, EGL_PBUFFER_BIT, EGL_NONE],
            ),
        ];

        for (name, attr) in filters {
            let _ = (egl.get_error)();
            nc = 0i32;
            cfg = std::ptr::null_mut();
            let ok = (egl.choose_config)(
                dpy,
                attr.as_ptr(),
                &mut cfg as *mut _ as *mut std::ffi::c_void,
                1,
                &mut nc,
            );
            let err = (egl.get_error)();
            if ok != 0 && nc >= 1 {
                log::info!("[glslang] eglChooseConfig({}) -> {} config", name, nc);
                chosen = true;
                break;
            } else {
                log::warn!(
                    "[glslang] eglChooseConfig({}) failed: ok={} nc={} err=0x{:x}",
                    name,
                    ok,
                    nc,
                    err
                );
            }
        }

        if !chosen {
            log::warn!("[glslang] all eglChooseConfig filters failed, no usable config");
            (egl.terminate)(dpy);
            return false;
        }

        (egl.bind_api)(EGL_OPENGL_ES_API);

        let ctx_attr = [EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE];
        let ctx = (egl.create_context)(dpy, cfg, std::ptr::null_mut(), ctx_attr.as_ptr());
        if ctx.is_null() {
            log::warn!(
                "[glslang] eglCreateContext failed, err=0x{:x}",
                (egl.get_error)()
            );
            (egl.terminate)(dpy);
            return false;
        }

        // surfaceless：使用 EGL_NO_SURFACE
        // eglMakeCurrent 返回 EGLBoolean，== 0 表示失败
        if (egl.make_current)(dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx) == 0 {
            log::warn!(
                "[glslang] eglMakeCurrent failed, err=0x{:x}",
                (egl.get_error)()
            );
            (egl.destroy_context)(dpy, ctx);
            (egl.terminate)(dpy);
            return false;
        }

        log::info!("[glslang] surfaceless GLES 3 context ready");
        true
    })
}

/// 用 GLES 后端编译 GLSL ES 源码，验证翻译结果可在真实驱动上编译。
///
/// 返回 `Ok(())` 表示编译成功，`Err(msg)` 表示编译失败（含 info log）。
/// 如果 GLES 后端不可用，返回 `Err("GLES backend unavailable".into())`。
pub fn gles_compile_check(source: &str, stage: u32) -> Result<(), String> {
    use crate::backend;

    let dispatch = match backend::gles_dispatch() {
        Some(d) => d,
        None => return Err("GLES backend unavailable".into()),
    };

    // 检查是否为 stub（GLES 库未加载）
    if dispatch.create_shader as *const () == dispatch.stub as *const () {
        return Err("GLES backend unavailable".into());
    }

    let c_source = match CString::new(source) {
        Ok(c) => c,
        Err(_) => return Err("source contains null byte".into()),
    };

    unsafe {
        let shader = (dispatch.create_shader)(stage);
        if shader == 0 {
            return Err("glCreateShader returned 0".into());
        }

        let ptr = c_source.as_ptr();
        let len = c_source.as_bytes().len() as i32;
        (dispatch.shader_source)(shader, 1, &ptr, &len);
        (dispatch.compile_shader)(shader);

        let mut status = 0i32;
        const GL_COMPILE_STATUS: u32 = 0x8B81;
        const GL_INFO_LOG_LENGTH: u32 = 0x8B84;
        (dispatch.get_shader_iv)(shader, GL_COMPILE_STATUS, &mut status);

        let result = if status == 0 {
            let mut log_len = 0i32;
            (dispatch.get_shader_iv)(shader, GL_INFO_LOG_LENGTH, &mut log_len);
            let mut buf = vec![0u8; log_len.max(1) as usize];
            let mut written = 0i32;
            (dispatch.get_shader_info_log)(
                shader,
                log_len,
                &mut written,
                buf.as_mut_ptr() as *mut std::ffi::c_char,
            );
            let info = String::from_utf8_lossy(&buf[..written.max(0) as usize]);
            Err(info.trim().to_string())
        } else {
            Ok(())
        };

        (dispatch.delete_shader)(shader);
        result
    }
}
