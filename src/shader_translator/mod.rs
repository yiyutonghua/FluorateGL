//! 着色器翻译模块
//!
//! 将桌面 OpenGL GLSL 翻译为 OpenGL ES GLSL，分为以下子模块：
//! - `preprocess`：GLSL 预处理（移除 #line、版本升级、注入 location/binding）
//! - `spirv_compile`：GLSL → SPIR-V 编译（glslang）
//! - `spirv_pass`：翻译管线编排 + SPIR-V 中间处理 Pass
//! - `gles_compile`：SPIR-V → GLSL ES 编译（spirv-cross2）
//! - `postprocess`：GLSL ES 后处理（移除 binding、处理 outColor、precision）
//! - `string_pass`：纯文本替换翻译（备用方案）

pub mod gles_compile;
pub mod postprocess;
pub mod preprocess;
pub mod spirv_compile;
pub mod spirv_pass;
pub mod string_pass;
