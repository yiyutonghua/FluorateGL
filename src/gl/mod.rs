//! 桌面 OpenGL 函数拦截层
//!
//! 通过 `#[no_mangle] extern "C"` 导出与桌面 GL 同名的符号，由 LD_PRELOAD
//! 拦截宿主调用。各子模块按 GL 对象类别组织：
//!
//! - [`buffer`] / [`vertex_array`] / [`texture`] / [`framebuffer`] / [`query`] /
//!   [`sync`]：对象生命周期与绑定管理，负责 desktop ↔ GLES ID 翻译
//! - [`shader`] / [`program`]：着色器编译/链接/查询，集成翻译管线
//! - [`drawing`]：draw call 分发与降级（`glDrawRangeElements` → `glDrawElements`、
//!   BaseVertex/BaseInstance/Indirect 系列的 stub 降级）
//! - [`multi_draw`]：`glMultiDraw*` 系列（含 Indirect/IndirectCount）
//! - [`render_state`]：混合/深度/模板等可编程管线状态
//! - [`getter`]：`glGet*` 系列查询
//! - [`pixel`]：像素/点参数（`glClampColor` stub、`glPointParameter*` 转发）
//! - [`exports`]：通用导出（`glClear` 等）与 GLES 不支持 cap 的过滤
//!
//! 所有拦截函数通过 [`crate::backend`] 取得 GLES dispatch 后转发，必要时做
//! 参数翻译（ID 映射、shader 源码翻译、不支持枚举过滤等）。

pub mod buffer;
pub mod drawing;
pub mod exports;
pub mod framebuffer;
pub mod getter;
pub mod multi_draw;
pub mod pixel;
pub mod program;
pub mod query;
pub mod render_state;
pub mod sampler;
pub mod shader;
pub mod sync;
pub mod texture;
pub mod transform_feedback;
pub mod vertex_array;
