//! EGL 函数拦截层
//!
//! 通过 `#[no_mangle] extern "C"` 导出与 EGL 同名的符号。[`exports`] 中的函数
//! 做以下处理：
//!
//! - `eglQueryString`：对 `EGL_VERSION` / `EGL_EXTENSIONS` 返回 FluorateGL 自有声明
//! - `eglCreateContext`：将桌面 OpenGL context 属性（profile/reset strategy）改写
//!   为 GLES 兼容属性，强制 `EGL_CONTEXT_CLIENT_VERSION = 3`
//! - `eglGetProcAddress`：优先返回本库拦截函数指针（通过 `init::get_self_handle`），
//!   确保宿主查询 GL 扩展函数时拿到的是 FluorateGL 的拦截实现
//! - 其余函数透传给底层 EGL 驱动

pub mod exports;
