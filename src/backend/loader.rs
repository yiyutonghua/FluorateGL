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

    pub fn get_proc(&self, name: &str) -> *mut c_void {
        let c_name = CString::new(name).unwrap();
        unsafe { dlsym(self.handle, c_name.as_ptr()) }
    }
}
