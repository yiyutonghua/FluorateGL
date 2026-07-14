use std::ffi::c_void;

#[allow(dead_code)]
pub struct EglDispatch {
    pub get_display: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub initialize: unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> u32,
    pub terminate: unsafe extern "C" fn(*mut c_void) -> u32,
    pub bind_api: unsafe extern "C" fn(u32) -> u32,
    pub choose_config: unsafe extern "C" fn(*mut c_void, *const i32, *mut c_void, i32, *mut i32) -> u32,
    pub get_config_attrib: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub create_window_surface: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub create_pbuffer_surface: unsafe extern "C" fn(*mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub destroy_surface: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
    pub create_context: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void,
    pub destroy_context: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
    pub make_current: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32,
    pub query_context: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub query_surface: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut i32) -> u32,
    pub swap_buffers: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32,
    pub swap_interval: unsafe extern "C" fn(*mut c_void, i32) -> u32,
    pub get_error: unsafe extern "C" fn() -> u32,
    pub get_proc_address: unsafe extern "C" fn(*const i8) -> *mut c_void,
    pub query_string: unsafe extern "C" fn(*mut c_void, i32) -> *const i8,
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

        Some(Self {
            get_display: unsafe { std::mem::transmute(load!("eglGetDisplay")) },
            initialize: unsafe { std::mem::transmute(load!("eglInitialize")) },
            bind_api: unsafe { std::mem::transmute(load!("eglBindAPI")) },
            choose_config: unsafe { std::mem::transmute(load!("eglChooseConfig")) },
            create_context: unsafe { std::mem::transmute(load!("eglCreateContext")) },
            make_current: unsafe { std::mem::transmute(load!("eglMakeCurrent")) },
            swap_buffers: unsafe { std::mem::transmute(load!("eglSwapBuffers")) },
            query_string: unsafe { std::mem::transmute(load!("eglQueryString")) },
            terminate: unsafe { std::mem::transmute(load!("eglTerminate")) },
            get_config_attrib: unsafe { std::mem::transmute(load!("eglGetConfigAttrib")) },
            create_window_surface: unsafe { std::mem::transmute(load!("eglCreateWindowSurface")) },
            create_pbuffer_surface: unsafe { std::mem::transmute(load!("eglCreatePbufferSurface")) },
            destroy_surface: unsafe { std::mem::transmute(load!("eglDestroySurface")) },
            destroy_context: unsafe { std::mem::transmute(load!("eglDestroyContext")) },
            query_context: unsafe { std::mem::transmute(load!("eglQueryContext")) },
            query_surface: unsafe { std::mem::transmute(load!("eglQuerySurface")) },
            swap_interval: unsafe { std::mem::transmute(load!("eglSwapInterval")) },
            get_error: unsafe { std::mem::transmute(load!("eglGetError")) },
            get_proc_address: unsafe { std::mem::transmute(load!("eglGetProcAddress")) },

        })
    }
}
