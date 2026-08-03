use crate::backend;
use crate::state;
use libc::c_char;
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};

/// glCreateShader 返回 0（无 EGL 上下文）首次告警标志
/// 触发场景：异步加载线程在 EGL 上下文创建前调用 GL
static SHADER_NO_CONTEXT_WARNED: AtomicBool = AtomicBool::new(false);
/// glShaderSource 参数非法首次告警标志
static SHADER_INVALID_ARGS_WARNED: AtomicBool = AtomicBool::new(false);
/// glGetShaderiv shader 不在 IdMap 中首次告警标志
/// 触发场景：跨线程查询或 shader 已被释放
static SHADER_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glCreateShader GLES 返回 0（无 EGL 上下文）。
fn warn_shader_no_context(shader_type: u32) {
    if !SHADER_NO_CONTEXT_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glCreateShader(0x{:04X}) -> GLES returned 0 (no context on tid={}, 后续调用将静默返回 0)",
            shader_type,
            state::thread_id_u64()
        );
    }
}

/// 首次告警：glShaderSource 参数非法。
fn warn_shader_invalid_args(string: *const *const c_char, count: i32) {
    if !SHADER_INVALID_ARGS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glShaderSource: invalid args (string={:?}, count={}, 后续调用将静默跳过)",
            string,
            count
        );
    }
}

/// 首次告警：glGetShaderiv shader 不在 IdMap 中。
fn warn_shader_id_miss(shader: u32) {
    if !SHADER_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glGetShaderiv: shader {} not found in IdMap (tid={}, params untouched, caller sees GL_FALSE, 后续将静默降级)",
            shader,
            state::thread_id_u64()
        );
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCreateShader(shader_type: u32) -> u32 {
    log::debug!("[FluorateGL] glCreateShader(0x{:04X})", shader_type);
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = (dispatch.create_shader)(shader_type);
        if gles_id == 0 {
            // GLES 返回 0 表示当前线程无 EGL 上下文（如异步加载线程）。
            // 直接返回 0，不分配 desktop_id，避免后续操作映射到无效的 gles_id=0。
            warn_shader_no_context(shader_type);
            return 0;
        }
        let desktop_id = state::with_state(|s| {
            let id = s.shaders.alloc(gles_id);
            s.shader_types.insert(id, shader_type);
            id
        });
        desktop_id
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteShader(shader: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| {
            s.shader_types.remove(&shader);
            s.shader_sources.remove(&shader);
            s.shader_original_sources.remove(&shader);
            s.shaders.delete(shader)
        });
        if let Some(gles_id) = gles_id {
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
        let (gles_id, stage) = state::with_state_ref(|s| {
            let gles_id = s.shaders.get_gles(shader).unwrap_or(0);
            let stage = s.shader_types.get(&shader).copied().unwrap_or(0);
            (gles_id, stage)
        });
        if gles_id == 0 {
            return;
        }

        // string 为 null 或 count <= 0 时无源码可上传，直接返回
        if string.is_null() || count <= 0 {
            warn_shader_invalid_args(string, count);
            return;
        }

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

        // 直接走完整翻译管线（全局 ShaderCache LruCache(64) 已兜底缓存语义）
        use crate::shader_translator::spirv_pass::TranslationResult;
        let translate_start = std::time::Instant::now();
        let (upload_source, translated) = match crate::shader_translator::spirv_pass::translate(
            &source, stage,
        ) {
            TranslationResult::Translated(translated) => {
                log::debug!(
                    "[ShaderTranslator] shader {} stage 0x{:04X} translated via SPIR-V ({} chars, took {:?})",
                    shader,
                    stage,
                    translated.len(),
                    translate_start.elapsed()
                );
                (translated, true)
            }
            TranslationResult::PassThrough => {
                // 预留分支：当前管线（Vulkan target + string_pass 兜底）不产生 PassThrough
                log::debug!(
                    "[ShaderTranslator] shader {} stage 0x{:04X} passed through unchanged",
                    shader,
                    stage
                );
                (source.clone(), false)
            }
            TranslationResult::Failed => {
                // 不可达：translate() 不变式保证永不返回 Failed（失败统一回退
                // string_pass，见 spirv_pass.rs）
                log::warn!(
                    "[ShaderTranslator] SPIR-V pipeline failed for shader {}; passing original source ({} chars, took {:?})",
                    shader,
                    source.len(),
                    translate_start.elapsed()
                );
                (source.clone(), false)
            }
        };

        state::with_state(|s| {
            if translated {
                s.shader_sources.insert(shader, upload_source.clone());
            } else {
                s.shader_sources.remove(&shader);
            }
            // 将真正的原始源码 source 移入 original_sources
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
    log::debug!("[FluorateGL] glCompileShader({})", shader);
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.shaders.get_gles(shader).unwrap_or(0));
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
                // 防御 Adreno 驱动 bug：written 可能 > len，截断避免越界
                let safe_written = (written.max(0) as usize).min(buf.len());
                let info = String::from_utf8_lossy(&buf[..safe_written]);
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

            if let Some(src) = state::with_state_ref(|s| s.shader_sources.get(&shader).cloned()) {
                log::error!(
                    "[FluorateGL] Translated source for shader {} ({} chars):\n{}",
                    shader,
                    src.len(),
                    src
                );
            }
            if let Some(src) =
                state::with_state_ref(|s| s.shader_original_sources.get(&shader).cloned())
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
        let gles_id = state::with_state_ref(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
            // shader 不在 IdMap 中：可能是跨线程查询（异步线程创建、Render 线程查询）
            // 或 glCreateShader 时底层 GLES 返回 0（当前线程无 EGL 上下文），此时
            // 不分配 desktop_id 直接返回 0，故该 id 不会进入 IdMap。
            // 本分支不设置 *params，调用方看到 0（GL_FALSE）。
            warn_shader_id_miss(shader);
            return;
        }

        // Return the length of the original source for GL_SHADER_SOURCE_LENGTH.
        if pname == GL_SHADER_SOURCE_LENGTH {
            let len = state::with_state_ref(|s| {
                s.shader_original_sources
                    .get(&shader)
                    .map(|src| src.len() as i32 + 1)
                    .unwrap_or(0)
            });
            *params = len;
            return;
        }
        (dispatch.get_shader_iv)(gles_id, pname, params);

        // fail-fast: 真实返回 compile 状态，不欺骗为 GL_TRUE。
        // 保留 error 级诊断日志，让失败有迹可循，便于定位 SPIR-V 翻译根因。
        if pname == GL_COMPILE_STATUS && *params == 0 {
            // 主动获取 GLES 编译错误信息，便于诊断翻译后源码的编译问题
            // 注意：不同平台 gl_bindings 的签名为 *mut c_char（i8 或 u8），用 cast 兼容
            let mut info_buf = [0u8; 4096];
            let mut info_len: i32 = 0;
            (dispatch.get_shader_info_log)(
                gles_id,
                info_buf.len() as i32,
                &mut info_len,
                info_buf.as_mut_ptr() as *mut _,
            );
            let info_str = if info_len > 0 {
                // 防御 Adreno 驱动 bug：info_len 可能 > 4096，截断避免越界
                let safe_len = (info_len as usize).min(info_buf.len());
                String::from_utf8_lossy(&info_buf[..safe_len]).to_string()
            } else {
                "(empty)".to_string()
            };
            // 输出翻译后源码前 500 字符，便于定位编译错误位置
            let translated_preview: String = state::with_state_ref(|s| {
                s.shader_sources
                    .get(&shader)
                    .map(|src| src.chars().take(500).collect::<String>())
                    .unwrap_or_default()
            });
            log::error!(
                "[FluorateGL] Shader {} (GLES {}) compile failed (fail-fast, returning GL_FALSE)\n  GLES info log: {}\n  Translated source (first 500 chars):\n{}",
                shader,
                gles_id,
                info_str,
                translated_preview
            );
        }
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
        let gles_id = state::with_state_ref(|s| s.shaders.get_gles(shader).unwrap_or(0));
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

    let original = state::with_state_ref(|s| s.shader_original_sources.get(&shader).cloned());
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
        let gles_id = state::with_state_ref(|s| s.shaders.get_gles(shader).unwrap_or(0));
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

    let shader_id = glCreateShader(shader_type);
    if shader_id == 0 {
        return 0;
    }

    // 手动上传源码（这里会触发我们的 SPIR-V 翻译管线！）
    glShaderSource(shader_id, count, strings, std::ptr::null());

    glCompileShader(shader_id);

    let mut status = 0i32;
    glGetShaderiv(shader_id, 0x8B81 /* GL_COMPILE_STATUS */, &mut status);

    if status == 0 {
        log::error!("[FluorateGL] glCreateShaderProgramv: Shader compilation failed internally.");
        // 即使失败也要创建 program 返回，否则 MC 会崩溃
    }

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

    // 清理 Shader 对象（glCreateShaderProgramv 规范要求隐式删除 shader）
    glDeleteShader(shader_id);

    program_id
}
