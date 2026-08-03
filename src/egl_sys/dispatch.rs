use libc::c_char;
use std::ffi::c_void;

#[allow(dead_code)]
#[repr(C)]
pub struct EglDispatch {
    pub get_display: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub initialize: unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> u32,
    pub terminate: unsafe extern "C" fn(*mut c_void) -> u32,
    pub query_string: unsafe extern "C" fn(*mut c_void, i32) -> *const c_char,
    pub get_configs: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub choose_config:
        unsafe extern "C" fn(*mut c_void, *const i32, *mut c_void, i32, *mut i32) -> u32,
    pub get_config_attrib: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub create_window_surface:
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub create_pbuffer_surface:
        unsafe extern "C" fn(*mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub create_pbuffer_from_client_buffer:
        unsafe extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub create_pixmap_surface:
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub destroy_surface: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
    pub surface_attrib: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, i32) -> u32,
    pub bind_tex_image: unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> u32,
    pub release_tex_image: unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> u32,
    pub create_context:
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub destroy_context: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
    pub make_current:
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32,
    pub query_context: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub query_surface: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub get_current_context: unsafe extern "C" fn() -> *mut c_void,
    pub get_current_surface: unsafe extern "C" fn(i32) -> *mut c_void,
    pub get_current_display: unsafe extern "C" fn() -> *mut c_void,
    pub wait_client: unsafe extern "C" fn() -> u32,
    pub wait_native: unsafe extern "C" fn(i32) -> u32,
    pub wait_gl: unsafe extern "C" fn() -> u32,
    pub release_thread: unsafe extern "C" fn() -> u32,
    pub bind_api: unsafe extern "C" fn(u32) -> u32,
    pub query_api: unsafe extern "C" fn() -> u32,
    pub swap_buffers: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
    pub swap_interval: unsafe extern "C" fn(*mut c_void, i32) -> u32,
    pub copy_buffers: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> u32,
    pub get_error: unsafe extern "C" fn() -> u32,
    pub get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void,
}

// 编译期断言：EglDispatch 共 34 个字段且全部为函数指针（#[repr(C)] 无 padding），
// 表大小必须恰好等于 字段数 × 函数指针大小。
// ⚠️ 字段数变化时必须同步更新 EGL_DISPATCH_FIELD_COUNT。
const EGL_DISPATCH_FIELD_COUNT: usize = 34;
const _: () = assert!(
    std::mem::size_of::<EglDispatch>()
        == EGL_DISPATCH_FIELD_COUNT * std::mem::size_of::<unsafe extern "C" fn()>()
);

// —— 按签名类别的安全 no-op stub（零参数，忽略入参，返回安全常量）——
// 用于 all_stub()：宿主在 stub 模式下调用时，拿到的是安全值而非寄存器垃圾。
unsafe extern "C" fn stub_zero_u32() -> u32 {
    0
}
unsafe extern "C" fn stub_null_ptr() -> *mut c_void {
    std::ptr::null_mut()
}
unsafe extern "C" fn stub_true() -> u32 {
    1 // EGL_TRUE
}
unsafe extern "C" fn stub_empty_string() -> *const c_char {
    b"\0".as_ptr() as *const c_char
}

/// 按签名类别将零参数 stub（先 reify 为自身签名的 fn pointer）transmute 为
/// 目标字段签名。stub 不使用入参（忽略寄存器/栈上的参数），转换安全。
///
/// 签名转换约束（重要）：
/// - 仅限返回 u32 / 指针 / void 的 C ABI 函数签名（stub_zero_u32 / stub_null_ptr / stub_empty_string / stub_true）
/// - 禁止用于返回 u64 / f64 等 8 字节非整数类型的字段——transmute 位级合法但 stub 只写 w0/x0 低 32 位，高 32 位残留 → UB
/// - 添加新 stub 签名类型时必须同步更新此注释与下方 stub 函数族
macro_rules! stub {
    ($e:expr) => {{
        unsafe { std::mem::transmute::<_, _>($e) }
    }};
}

impl EglDispatch {
    /// Create a dispatch table where every function pointer is a safe no-op stub.
    ///
    /// 按签名类别返回安全值（P1-A：避免宿主把 AArch64 x0 残留垃圾当指针/状态使用）：
    /// - 返回指针（创建/获取类）→ `stub_null_ptr`（null，宿主可安全判空）
    /// - 返回 u32 状态/查询类 → `stub_zero_u32`（0 = EGL_FALSE / 失败码）
    /// - 返回 EGLBoolean 类（bind_api）→ `stub_true`（EGL_TRUE = 1）
    /// - 返回字符串类（query_string）→ `stub_empty_string`（空串，宿主可安全 CStr 解析）
    pub fn all_stub() -> Self {
        Self {
            get_display: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            initialize: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            terminate: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            query_string: stub!(stub_empty_string as unsafe extern "C" fn() -> *const c_char),
            get_configs: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            choose_config: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            get_config_attrib: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            create_window_surface: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            create_pbuffer_surface: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            create_pbuffer_from_client_buffer: stub!(
                stub_null_ptr as unsafe extern "C" fn() -> *mut c_void
            ),
            create_pixmap_surface: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            destroy_surface: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            surface_attrib: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            bind_tex_image: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            release_tex_image: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            create_context: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            destroy_context: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            make_current: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            query_context: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            query_surface: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            get_current_context: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            get_current_surface: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            get_current_display: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            wait_client: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            wait_native: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            wait_gl: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            release_thread: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            bind_api: stub!(stub_true as unsafe extern "C" fn() -> u32), // EGLBoolean = EGL_TRUE
            query_api: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            swap_buffers: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            swap_interval: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            copy_buffers: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            get_error: stub!(stub_zero_u32 as unsafe extern "C" fn() -> u32),
            get_proc_address: stub!(stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
        }
    }

    #[allow(clippy::missing_transmute_annotations)]
    pub fn load_from(loader: &super::loader::EglLoader) -> Option<Self> {
        macro_rules! load {
            ($name:expr) => {{
                let ptr = loader.get_proc($name);
                if ptr.is_null() {
                    return None;
                }
                ptr
            }};
        }
        macro_rules! load_opt {
            ($name:expr) => {{
                let ptr = loader.get_proc($name);
                if ptr.is_null() {
                    log::warn!(
                        "[EglDispatch] warning: optional function not available: {}",
                        $name
                    );
                    unimplemented_stub as *mut c_void
                } else {
                    ptr
                }
            }};
        }

        unsafe extern "C" fn unimplemented_stub() {}

        Some(Self {
            get_display: unsafe { std::mem::transmute(load!("eglGetDisplay")) },
            initialize: unsafe { std::mem::transmute(load!("eglInitialize")) },
            terminate: unsafe { std::mem::transmute(load!("eglTerminate")) },
            query_string: unsafe { std::mem::transmute(load!("eglQueryString")) },
            get_configs: unsafe { std::mem::transmute(load!("eglGetConfigs")) },
            choose_config: unsafe { std::mem::transmute(load!("eglChooseConfig")) },
            get_config_attrib: unsafe { std::mem::transmute(load!("eglGetConfigAttrib")) },
            create_window_surface: unsafe { std::mem::transmute(load!("eglCreateWindowSurface")) },
            create_pbuffer_surface: unsafe {
                std::mem::transmute(load_opt!("eglCreatePbufferSurface"))
            },
            create_pbuffer_from_client_buffer: unsafe {
                std::mem::transmute(load_opt!("eglCreatePbufferFromClientBuffer"))
            },
            create_pixmap_surface: unsafe {
                std::mem::transmute(load_opt!("eglCreatePixmapSurface"))
            },
            destroy_surface: unsafe { std::mem::transmute(load!("eglDestroySurface")) },
            surface_attrib: unsafe { std::mem::transmute(load_opt!("eglSurfaceAttrib")) },
            bind_tex_image: unsafe { std::mem::transmute(load_opt!("eglBindTexImage")) },
            release_tex_image: unsafe { std::mem::transmute(load_opt!("eglReleaseTexImage")) },
            create_context: unsafe { std::mem::transmute(load!("eglCreateContext")) },
            destroy_context: unsafe { std::mem::transmute(load!("eglDestroyContext")) },
            make_current: unsafe { std::mem::transmute(load!("eglMakeCurrent")) },
            query_context: unsafe { std::mem::transmute(load!("eglQueryContext")) },
            query_surface: unsafe { std::mem::transmute(load!("eglQuerySurface")) },
            get_current_context: unsafe { std::mem::transmute(load!("eglGetCurrentContext")) },
            get_current_surface: unsafe { std::mem::transmute(load!("eglGetCurrentSurface")) },
            get_current_display: unsafe { std::mem::transmute(load!("eglGetCurrentDisplay")) },
            wait_client: unsafe { std::mem::transmute(load_opt!("eglWaitClient")) },
            wait_native: unsafe { std::mem::transmute(load_opt!("eglWaitNative")) },
            wait_gl: unsafe { std::mem::transmute(load_opt!("eglWaitGL")) },
            release_thread: unsafe { std::mem::transmute(load_opt!("eglReleaseThread")) },
            bind_api: unsafe { std::mem::transmute(load!("eglBindAPI")) },
            query_api: unsafe { std::mem::transmute(load_opt!("eglQueryAPI")) },
            swap_buffers: unsafe { std::mem::transmute(load!("eglSwapBuffers")) },
            swap_interval: unsafe { std::mem::transmute(load!("eglSwapInterval")) },
            copy_buffers: unsafe { std::mem::transmute(load_opt!("eglCopyBuffers")) },
            get_error: unsafe { std::mem::transmute(load!("eglGetError")) },
            get_proc_address: unsafe { std::mem::transmute(load!("eglGetProcAddress")) },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1-A：all_stub() 各字段必须按签名类别指向对应 stub
    /// （比较裸指针地址，避免签名不同的函数指针直接比较）
    #[test]
    fn all_stub_assigns_safe_values_by_signature() {
        let s = EglDispatch::all_stub();

        // 返回指针类（创建/获取类）→ stub_null_ptr（null 安全值）
        assert_eq!(s.get_display as *const (), stub_null_ptr as *const ());
        assert_eq!(s.create_context as *const (), stub_null_ptr as *const ());
        assert_eq!(s.create_window_surface as *const (), stub_null_ptr as *const ());
        assert_eq!(s.create_pbuffer_surface as *const (), stub_null_ptr as *const ());
        assert_eq!(
            s.create_pbuffer_from_client_buffer as *const (),
            stub_null_ptr as *const ()
        );
        assert_eq!(s.create_pixmap_surface as *const (), stub_null_ptr as *const ());
        assert_eq!(s.get_current_context as *const (), stub_null_ptr as *const ());
        assert_eq!(s.get_current_surface as *const (), stub_null_ptr as *const ());
        assert_eq!(s.get_current_display as *const (), stub_null_ptr as *const ());
        assert_eq!(s.get_proc_address as *const (), stub_null_ptr as *const ());

        // u32 状态/查询类 → stub_zero_u32（0 = EGL_FALSE / 失败码）
        assert_eq!(s.initialize as *const (), stub_zero_u32 as *const ());
        assert_eq!(s.terminate as *const (), stub_zero_u32 as *const ());
        assert_eq!(s.make_current as *const (), stub_zero_u32 as *const ());
        assert_eq!(s.query_context as *const (), stub_zero_u32 as *const ());
        assert_eq!(s.query_surface as *const (), stub_zero_u32 as *const ());
        assert_eq!(s.swap_buffers as *const (), stub_zero_u32 as *const ());
        assert_eq!(s.get_error as *const (), stub_zero_u32 as *const ());

        // EGLBoolean 类 → stub_true（EGL_TRUE = 1）
        assert_eq!(s.bind_api as *const (), stub_true as *const ());

        // 字符串类 → stub_empty_string（空串指针，非 null，可安全 CStr 解析）
        assert_eq!(s.query_string as *const (), stub_empty_string as *const ());
    }

    /// P1-A：运行时验证各签名类别 stub 返回的安全值本身正确
    #[test]
    fn all_stub_returns_safe_values() {
        let s = EglDispatch::all_stub();
        unsafe {
            // 指针类 → null
            assert!((s.get_display)(std::ptr::null_mut()).is_null());
            assert!(
                (s.create_context)(
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null()
                )
                .is_null()
            );
            // u32 状态类 → 0
            assert_eq!(
                (s.initialize)(
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut()
                ),
                0
            );
            // EGLBoolean 类 → 1
            assert_eq!((s.bind_api)(0), 1);
            // 字符串类 → 非 null 且首字节为 NUL（空串）
            let str_ptr = (s.query_string)(std::ptr::null_mut(), 0);
            assert!(!str_ptr.is_null());
            assert_eq!(*str_ptr, 0);
        }
    }
}
