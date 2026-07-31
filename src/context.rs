//! Surfaceless GLES 上下文（用于离线编译测试）
//!
//! 在不加载完整 MC 的环境下，创建一个 surfaceless GLES 3 上下文，用于验证
//! 翻译后的 GLSL ES 能否在真实 GLES 驱动上编译。仅在 EGL/GLES 后端可用时工作。
//!
//! 适配策略保证 Android / ANGLE / Linux surfaceless 三平台兼容：
//! 1. eglGetDisplay(EGL_DEFAULT_DISPLAY=NULL)
//!    - Android: 返回默认 display
//!    - Linux surfaceless: 配合 EGL_PLATFORM=surfaceless 返回软件 display
//!    - ANGLE: 返回 ANGLE display
//! 2. eglChooseConfig 依次尝试 GLES3 → GLES2 → 无过滤，适配各平台 config 能力
//! 3. eglCreateContext 指定 client version 3（GLES 3.0+，兼容 GLSL ES 300/310/320）

use std::sync::OnceLock;

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
