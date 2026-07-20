use crate::backend;
use crate::state;
use libc::c_char;
use std::ffi::{CStr, CString};

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCreateShader(shader_type: u32) -> u32 {
    log::debug!("[FluorateGL] glCreateShader(0x{:04X})", shader_type);
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = (dispatch.create_shader)(shader_type);
        let desktop_id = state::with_state(|s| s.shaders.alloc(gles_id));
        state::with_state(|s| {
            s.shader_types.insert(desktop_id, shader_type);
        });
        desktop_id
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteShader(shader: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        state::with_state(|s| {
            s.shader_types.remove(&shader);
            s.shader_sources.remove(&shader);
            s.shader_original_sources.remove(&shader);
        });
        if let Some(gles_id) = state::with_state(|s| s.shaders.delete(shader)) {
            (dispatch.delete_shader)(gles_id);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glShaderSource(
    shader: u32,
    count: i32,
    string: *const *const c_char,
    length: *const i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
            return;
        }

        // string 为 null 或 count <= 0 时无源码可上传，直接返回
        if string.is_null() || count <= 0 {
            log::warn!(
                "[FluorateGL] glShaderSource: invalid args (string={:?}, count={})",
                string, count
            );
            return;
        }

        let stage = state::with_state(|s| s.shader_types.get(&shader).copied().unwrap_or(0));

        // Concatenate all source strings.
        let mut source = String::new();
        for i in 0..count as isize {
            let ptr = *string.offset(i);
            if ptr.is_null() {
                continue;
            }
            let len = if length.is_null() {
                0
            } else {
                // length[i] < 0 表示该字符串以 null 结尾，按 0 处理走 CStr 路径
                let v = *length.offset(i);
                if v < 0 { 0 } else { v as usize }
            };
            let piece = if len == 0 {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            } else {
                let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
                String::from_utf8_lossy(bytes).into_owned()
            };
            source.push_str(&piece);
        }

        use crate::shader_translator::spirv_pass::TranslationResult;
        let (upload_source, translated) = match crate::shader_translator::spirv_pass::translate(
            &source, stage,
        ) {
            TranslationResult::Translated(translated) => {
                log::debug!(
                    "[ShaderTranslator] shader {} stage 0x{:04X} translated via SPIR-V ({} chars)",
                    shader,
                    stage,
                    translated.len()
                );
                (translated, true)
            }
            TranslationResult::PassThrough => {
                log::debug!(
                    "[ShaderTranslator] shader {} stage 0x{:04X} passed through unchanged (driver extension supported)",
                    shader,
                    stage
                );
                // ✅ 修复：使用 clone，避免 source 被 move
                (source.clone(), false)
            }
            TranslationResult::Failed => {
                log::warn!(
                    "[ShaderTranslator] SPIR-V pipeline failed for shader {}; passing original source ({} chars)",
                    shader,
                    source.len()
                );
                // ✅ 修复：使用 clone，避免 source 被 move
                (source.clone(), false)
            }
        };

        state::with_state(|s| {
            if translated {
                s.shader_sources.insert(shader, upload_source.clone());
            } else {
                s.shader_sources.remove(&shader);
            }
            // ✅ 修复：将真正的原始源码 source 移入 original_sources
            s.shader_original_sources.insert(shader, source);
        });

        let c_source = match CString::new(upload_source) {
            Ok(c) => c,
            Err(_) => {
                log::error!("[FluorateGL] shader source contains null byte, passing through");
                (dispatch.shader_source)(gles_id, count, string, length);
                return;
            }
        };

        let ptr = c_source.as_ptr();
        let len = c_source.as_bytes().len() as i32;
        (dispatch.shader_source)(gles_id, 1, &ptr, &len);
    });
}

const GL_COMPILE_STATUS: u32 = 0x8B81;
const GL_INFO_LOG_LENGTH: u32 = 0x8B84;
const GL_SHADER_SOURCE_LENGTH: u32 = 0x8B88;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCompileShader(shader: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
            return;
        }

        (dispatch.compile_shader)(gles_id);

        let mut status = 0i32;
        (dispatch.get_shader_iv)(gles_id, GL_COMPILE_STATUS, &mut status);
        if status == 0 {
            let mut len = 0i32;
            (dispatch.get_shader_iv)(gles_id, GL_INFO_LOG_LENGTH, &mut len);
            if len > 0 {
                let mut buf = vec![0u8; len as usize];
                let mut written = 0i32;
                (dispatch.get_shader_info_log)(
                    gles_id,
                    len,
                    &mut written,
                    buf.as_mut_ptr() as *mut c_char,
                );
                let info = String::from_utf8_lossy(&buf[..written.max(0) as usize]);
                log::error!(
                    "[FluorateGL] Shader {} (GLES {}) compile failed: {}",
                    shader,
                    gles_id,
                    info.trim()
                );
            } else {
                log::error!(
                    "[FluorateGL] Shader {} (GLES {}) compile failed (no info log)",
                    shader,
                    gles_id
                );
            }

            if let Some(src) = state::with_state(|s| s.shader_sources.get(&shader).cloned()) {
                log::error!(
                    "[FluorateGL] Translated source for shader {} ({} chars):\n{}",
                    shader,
                    src.len(),
                    src
                );
            }
            if let Some(src) =
                state::with_state(|s| s.shader_original_sources.get(&shader).cloned())
            {
                log::error!(
                    "[FluorateGL] Original source for shader {} ({} chars):\n{}",
                    shader,
                    src.len(),
                    src
                );
            }
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetShaderiv(shader: u32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
            return;
        }

        // Return the length of the original source for GL_SHADER_SOURCE_LENGTH.
        if pname == GL_SHADER_SOURCE_LENGTH {
            let len = state::with_state(|s| {
                s.shader_original_sources
                    .get(&shader)
                    .map(|src| src.len() as i32 + 1)
                    .unwrap_or(0)
            });
            *params = len;
            return;
        }
        (dispatch.get_shader_iv)(gles_id, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetShaderInfoLog(
    shader: u32,
    buf_size: i32,
    length: *mut i32,
    info_log: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_shader_info_log)(gles_id, buf_size, length, info_log);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetShaderSource(
    shader: u32,
    buf_size: i32,
    length: *mut i32,
    source: *mut c_char,
) {
    if source.is_null() || buf_size <= 0 {
        return;
    }

    let original = state::with_state(|s| s.shader_original_sources.get(&shader).cloned());
    let Some(src) = original else {
        unsafe {
            *source = 0;
        }
        if !length.is_null() {
            unsafe {
                *length = 0;
            }
        }
        return;
    };

    let bytes = src.as_bytes();
    let max_write = (buf_size - 1).max(0) as usize;
    let write_len = bytes.len().min(max_write);

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), source as *mut u8, write_len);
        *source.add(write_len) = 0;
    }

    if !length.is_null() {
        unsafe {
            *length = write_len as i32;
        }
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsShader(shader: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
            return 0;
        }
        (dispatch.is_shader)(gles_id)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glReleaseShaderCompiler() {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.release_shader_compiler)();
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCreateShaderProgramv(
    shader_type: u32,
    count: i32,
    strings: *const *const libc::c_char,
) -> u32 {
    log::debug!(
        "[FluorateGL] Intercepted glCreateShaderProgramv for stage 0x{:04X}",
        shader_type
    );

    // 1. 手动创建 Shader
    let shader_id = glCreateShader(shader_type);
    if shader_id == 0 {
        return 0;
    }

    // 2. 手动上传源码（这里会触发我们的 SPIR-V 翻译管线！）
    glShaderSource(shader_id, count, strings, std::ptr::null());

    // 3. 手动编译
    glCompileShader(shader_id);

    // 4. 检查编译状态
    let mut status = 0i32;
    glGetShaderiv(shader_id, 0x8B81 /* GL_COMPILE_STATUS */, &mut status);

    if status == 0 {
        log::error!("[FluorateGL] glCreateShaderProgramv: Shader compilation failed internally.");
        // 即使失败也要创建 program 返回，否则 MC 会崩溃
    }

    // 5. 创建 Program 并链接
    let program_id = backend::with_gles_dispatch(|dispatch| unsafe {
        let prog = (dispatch.create_program)();
        if prog == 0 {
            return 0;
        }

        // 获取底层的 GLES shader id
        let gles_shader = state::with_state(|s| s.shaders.get_gles(shader_id).unwrap_or(0));
        if gles_shader != 0 {
            (dispatch.attach_shader)(prog, gles_shader);
            (dispatch.link_program)(prog);
            (dispatch.detach_shader)(prog, gles_shader);
        }

        prog
    });

    // 6. 清理 Shader 对象（glCreateShaderProgramv 规范要求隐式删除 shader）
    glDeleteShader(shader_id);

    program_id
}
