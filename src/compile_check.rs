//! 离线 GLES 编译验证
//!
//! 用 GLES 后端编译 GLSL ES 源码，验证翻译结果可在真实驱动上编译。
//! 供 glslang test suite 等离线测试使用。

use std::ffi::CString;

/// 用 GLES 后端编译 GLSL ES 源码，验证翻译结果可在真实驱动上编译。
///
/// 返回 `Ok(())` 表示编译成功，`Err(msg)` 表示编译失败（含 info log）。
/// 如果 GLES 后端不可用，返回 `Err("GLES backend unavailable".into())`。
pub fn gles_compile_check(source: &str, stage: u32) -> Result<(), String> {
    let dispatch = match crate::backend::gles_dispatch() {
        Some(d) => d,
        None => return Err("GLES backend unavailable".into()),
    };

    // 检查是否为 stub（GLES 库未加载）
    if dispatch.create_shader as *const () == dispatch.stub as *const () {
        return Err("GLES backend unavailable".into());
    }

    let c_source = match CString::new(source) {
        Ok(c) => c,
        Err(_) => return Err("source contains null byte".into()),
    };

    unsafe {
        let shader = (dispatch.create_shader)(stage);
        if shader == 0 {
            return Err("glCreateShader returned 0".into());
        }

        let ptr = c_source.as_ptr();
        let len = c_source.as_bytes().len() as i32;
        (dispatch.shader_source)(shader, 1, &ptr, &len);
        (dispatch.compile_shader)(shader);

        let mut status = 0i32;
        const GL_COMPILE_STATUS: u32 = 0x8B81;
        const GL_INFO_LOG_LENGTH: u32 = 0x8B84;
        (dispatch.get_shader_iv)(shader, GL_COMPILE_STATUS, &mut status);

        let result = if status == 0 {
            let mut log_len = 0i32;
            (dispatch.get_shader_iv)(shader, GL_INFO_LOG_LENGTH, &mut log_len);
            let mut buf = vec![0u8; log_len.max(1) as usize];
            let mut written = 0i32;
            (dispatch.get_shader_info_log)(
                shader,
                log_len,
                &mut written,
                buf.as_mut_ptr() as *mut std::ffi::c_char,
            );
            let info = String::from_utf8_lossy(&buf[..written.max(0) as usize]);
            Err(info.trim().to_string())
        } else {
            Ok(())
        };

        (dispatch.delete_shader)(shader);
        result
    }
}
