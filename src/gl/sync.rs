use crate::backend;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFenceSync(condition: u32, flags: u32) -> *mut std::ffi::c_void {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.fence_sync)(condition, flags) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteSync(sync: *mut std::ffi::c_void) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.delete_sync)(sync);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClientWaitSync(
    sync: *mut std::ffi::c_void,
    flags: u32,
    timeout: u64,
) -> u32 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.client_wait_sync)(sync, flags, timeout) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glWaitSync(sync: *mut std::ffi::c_void, flags: u32, timeout: u64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.wait_sync)(sync, flags, timeout);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsSync(sync: *mut std::ffi::c_void) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.is_sync)(sync) })
}
