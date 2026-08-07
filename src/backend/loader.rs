use crate::config::Config;
use libc::{RTLD_NOW, dlopen, dlsym};
use std::ffi::{CString, c_void};

pub struct GlesLoader {
    handle: *mut c_void,
}

impl GlesLoader {
    pub fn new(config: &Config) -> Result<Self, &'static str> {
        let path = CString::new(config.gles_lib_name()).map_err(|_| "invalid path")?;
        let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW) };

        if handle.is_null() {
            return Err("failed to load GLES library");
        }

        Ok(Self { handle })
    }

    /// 纯 dlsym 加载（默认路径）。
    ///
    /// 不附加 eglGetProcAddress 兜底：load_opt! 加载的函数名同时包含桌面 GL
    /// 独有函数（glGetBufferSubData/glGetTexImage 等），Mesa EGL 的
    /// eglGetProcAddress 会返回**桌面 GL 入口**，在 GLES context 上调用产生
    /// GL_INVALID_OPERATION 且数据不写（实测 b00/b03/e14 回归）。这些桌面
    /// 函数在 GLES 中不存在，应走 stub → 模拟降级路径。
    pub fn get_proc(&self, name: &str) -> *mut c_void {
        let c_name = CString::new(name).unwrap();
        unsafe { dlsym(self.handle, c_name.as_ptr()) }
    }

    /// GLES 扩展/3.2 函数专用加载：dlsym 失败后兜底 eglGetProcAddress（C2）。
    ///
    /// 仅用于 load_opt_suffixes! 的 OES/EXT 后缀名（glMultiDrawArraysEXT 等
    /// GLES 扩展名，eglGetProcAddress 返回 GLES 入口）。core 名仍走纯 dlsym：
    /// core 名可能是桌面独有（如 glMultiDrawArrays），兜底会拿到桌面入口。
    /// 加载顺序保证：ensure_backend_initialized 先 init_egl 再 init_gles，
    /// EGL_DISPATCH 就绪时此处兜底可用；EGL 加载失败则返回 null（走 stub 降级）。
    pub fn get_proc_gles(&self, name: &str) -> *mut c_void {
        let c_name = CString::new(name).unwrap();
        let ptr = unsafe { dlsym(self.handle, c_name.as_ptr()) };
        if !ptr.is_null() {
            return ptr;
        }
        crate::backend::egl_dispatch().map_or(std::ptr::null_mut(), |d| unsafe {
            (d.get_proc_address)(c_name.as_ptr())
        })
    }
}
