//! FluorateGL：桌面 OpenGL → OpenGL ES 翻译层
//!
//! 通过 LD_PRELOAD 拦截宿主进程的 GL/EGL 调用，将桌面 GLSL 翻译为 GLSL ES
//! 后转发给底层 GLES 驱动。核心模块：
//!
//! - [`init`]：库初始化（`#[ctor]` 自动触发）与自身句柄捕获
//! - [`backend`]：EGL/GLES 库加载与函数指针分发
//! - [`gl`] / [`egl`]：GL/EGL 函数拦截导出（`#[no_mangle]` 符号）
//! - [`shader_translator`]：GLSL → SPIR-V → GLSL ES 翻译管线
//! - [`state`]：desktop ↔ GLES ID 映射与缓存
//! - [`context`]：离线测试用的 surfaceless GLES 上下文
//! - [`compile_check`]：离线 GLES 编译验证
//!
//! 详见各模块文档。

mod backend;
mod compile_check;
mod config;
mod context;
mod egl;
mod egl_sys;
mod gl;
mod init;
pub mod shader_translator;
mod state;
mod util;

// 对外公开的初始化与测试辅助 API（保持 crate 根路径兼容）
pub use compile_check::gles_compile_check;
pub use context::ensure_gles_context;
pub use init::VERSION;
pub use init::fluorategl_init;
pub use init::get_self_handle;
