//! 着色器翻译模块
//!
//! 将桌面 OpenGL GLSL 翻译为 OpenGL ES GLSL，分为以下子模块：
//! - `preprocess`：GLSL 预处理（移除 #line、版本升级、注入 location/binding）
//! - `spirv_compile`：GLSL → SPIR-V 编译（shaderc）
//! - `spirv_pass`：翻译管线编排 + SPIR-V 中间处理 Pass
//! - `gles_compile`：SPIR-V → GLSL ES 编译（spirv-cross2）
//! - `postprocess`：GLSL ES 后处理（移除 binding、处理 outColor、precision）
//! - `string_pass`：纯文本替换翻译（备用方案）

pub mod cache;
pub mod gles_compile;
pub mod macros;
pub mod postprocess;
pub mod preprocess;
pub mod spirv_compile;
pub mod spirv_pass;
pub mod string_pass;

// 注：被使用的宏（simplify_ubo_processing / simplify_vulkan_workarounds）
// 通过 #[macro_export] 导出到 crate 根，调用方用 `use crate::xxx;` 导入。
// 其余预留宏（warn_once/gl_check_error/check_id_mapping/is_stub/gles_dispatch/
// optimize_spirv_compile）暂无调用点，保留在 macros 模块内供后续使用。
