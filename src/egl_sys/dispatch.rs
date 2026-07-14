use std::ffi::c_void;

#[allow(dead_code)]
pub struct EglDispatch {
    pub get_display: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub initialize: unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> u32,
    pub terminate: unsafe extern "C" fn(*mut c_void) -> u32,
    pub query_string: unsafe extern "C" fn(*mut c_void, i32) -> *const i8,
    pub get_configs: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub choose_config: unsafe extern "C" fn(*mut c_void, *const i32, *mut c_void, i32, *mut i32) -> u32,
    pub get_config_attrib: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub create_window_surface: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub create_pbuffer_surface: unsafe extern "C" fn(*mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub create_pbuffer_from_client_buffer: unsafe extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub create_pixmap_surface: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub destroy_surface: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
    pub surface_attrib: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, i32) -> u32,
    pub bind_tex_image: unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> u32,
    pub release_tex_image: unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> u32,
    pub create_context: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub destroy_context: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
    pub make_current: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32,
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
    pub get_proc_address: unsafe extern "C" fn(*const i8) -> *mut c_void,
}

impl EglDispatch {
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
            create_pbuffer_surface: unsafe { std::mem::transmute(load_opt!("eglCreatePbufferSurface")) },
            create_pbuffer_from_client_buffer: unsafe { std::mem::transmute(load_opt!("eglCreatePbufferFromClientBuffer")) },
            create_pixmap_surface: unsafe { std::mem::transmute(load_opt!("eglCreatePixmapSurface")) },
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
