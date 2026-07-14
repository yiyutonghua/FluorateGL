pub mod loader;
pub mod dispatch;

use std::sync::OnceLock;
use crate::config::Config;

// 全局状态
static CONFIG: OnceLock<Config> = OnceLock::new();
pub static GLES_DISPATCH: OnceLock<dispatch::GlesDispatch> = OnceLock::new();
static EGL_DISPATCH: OnceLock<crate::egl_sys::dispatch::EglDispatch> = OnceLock::new();
static INIT_ONCE: OnceLock<()> = OnceLock::new();

pub fn ensure_initialized() {
    INIT_ONCE.get_or_init(|| {
        let _ = crate::fluorategl_init();
    });
}

pub fn set_config(config: Config) {
    let _ = CONFIG.set(config);
}

pub fn init_gles() -> Result<(), &'static str> {
    let config = CONFIG.get().expect("config not set");
    
    let loader = loader::GlesLoader::new(config)?;
    let dispatch = dispatch::GlesDispatch::load_from(&loader)
        .ok_or("failed to load required GLES function")?;
    
    let _ = GLES_DISPATCH.set(dispatch);
    
    Box::leak(Box::new(loader));
    Ok(())
}

pub fn init_egl() -> Result<(), &'static str> {
    let config = CONFIG.get().expect("config not set");
    
    let loader = crate::egl_sys::loader::EglLoader::new(config)?;
    let dispatch = crate::egl_sys::dispatch::EglDispatch::load_from(&loader)
        .ok_or("failed to load required EGL function")?;
    
    let _ = EGL_DISPATCH.set(dispatch);
    
    Box::leak(Box::new(loader));
    Ok(())
}

pub fn with_gles_dispatch<F, R>(f: F) -> R
where
    F: FnOnce(&dispatch::GlesDispatch) -> R,
{
    ensure_initialized();
    let dispatch = GLES_DISPATCH.get().expect("GLES not initialized");
    f(dispatch)
}

pub fn with_egl_dispatch<F, R>(f: F) -> R
where
    F: FnOnce(&crate::egl_sys::dispatch::EglDispatch) -> R,
{
    ensure_initialized();
    let dispatch = EGL_DISPATCH.get().expect("EGL not initialized");
    f(dispatch)
}
