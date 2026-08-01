use crate::backend;
use crate::gl::buffer::sync_persistent_buffer_if_needed;
use crate::gl::getter;
use crate::state;
use libc::c_char;
use std::ffi::CString;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// GL_ARRAY_BUFFER target
const GL_ARRAY_BUFFER: u32 = 0x8892;
/// GL_ELEMENT_ARRAY_BUFFER target
const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;

/// glGetIntegerv 绑定查询时 GLES ID 未在 IdMap 中找到首次告警标志
static BINDING_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glGetIntegerv 绑定查询 GLES ID 未在 IdMap 中找到。
fn warn_binding_id_miss(pname: u32, gles_id: u32) {
    if !BINDING_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glGetIntegerv(0x{:04X}): GLES ID {} not found in IdMap, returning raw GLES ID (跨线程或资源已释放，后续将静默返回原始 GLES ID)",
            pname,
            gles_id
        );
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClear(mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear)(mask);
    });
}

// glAlphaFunc 是桌面 GL 固定功能，GLES 2.0+ 不支持，alpha test 在 shader 中通过 discard 实现
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glAlphaFunc(_func: u32, _ref: f32) {}

// Capabilities that exist in desktop GL but are unsupported (or always on)
// in OpenGL ES. Passing them to GLES produces `GL_INVALID_ENUM`.
pub(crate) fn is_unsupported_gles_cap(cap: u32) -> bool {
    matches!(
        cap,
        0x884F | // GL_TEXTURE_CUBE_MAP_SEAMLESS
        0x8642 | // GL_PROGRAM_POINT_SIZE
        0x0B10 | // GL_POINT_SMOOTH
        0x0B20 | // GL_LINE_SMOOTH
        0x0B41 | // GL_POLYGON_SMOOTH
        0x809D | // GL_MULTISAMPLE
        0x0B21 | // GL_LINE_STIPPLE
        0x0BC0 // GL_ALPHA_TEST
    )
}

// GL_DEBUG_OUTPUT：MC(blaze3d) 会启用 KHR_debug 回调抓驱动消息，但 Adreno 驱动会刷出
// 大量 PERFORMANCE 噪声（glDebugMessageControl 对 HIGH 级 PERFORMANCE 过滤无效）。
// suppress_debug_noise 已 glDisable(GL_DEBUG_OUTPUT)，这里吞掉 MC 的重新启用，保持彻底关闭。
const GL_DEBUG_OUTPUT: u32 = 0x9146;

/// glDebugMessageCallback stub — 吞掉 MC/LWJGL 注册的 KHR_debug 回调。
///
/// Adreno 驱动无视 glDisable(GL_DEBUG_OUTPUT) 和 glDebugMessageControl 过滤，
/// 在回调注册后仍持续发送 PERFORMANCE 消息（"Packing allocations" 等），
/// 每条触发 LWJGL 的 Java 堆栈转储，导致 OptiFine 纹理图集上传阶段极端缓慢。
///
/// 不注册任何回调（直接吞掉），驱动即使生成消息也无回调可调用，彻底阻断噪声。
/// 同时避免 MC 绕过拦截层通过 dlsym 找到 GLES 驱动的真实 glDebugMessageCallback。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDebugMessageCallback(
    _callback: *const std::ffi::c_void,
    _user_param: *const std::ffi::c_void,
) {
    log::debug!(
        "[FluorateGL] glDebugMessageCallback swallowed (callback not registered, driver debug noise blocked)"
    );
}

/// glDebugMessageCallbackKHR stub — 与 glDebugMessageCallback 等价的 KHR 扩展入口。
///
/// 部分 LWJGL 版本优先查询 KHR 后缀版本，提供此入口确保两条查询路径都被拦截。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDebugMessageCallbackKHR(
    _callback: *const std::ffi::c_void,
    _user_param: *const std::ffi::c_void,
) {
    log::debug!(
        "[FluorateGL] glDebugMessageCallbackKHR swallowed (callback not registered, driver debug noise blocked)"
    );
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEnable(cap: u32) {
    if cap == GL_DEBUG_OUTPUT {
        log::debug!(
            "[FluorateGL] glEnable(GL_DEBUG_OUTPUT) swallowed (driver debug noise suppressed)"
        );
        return;
    }
    if is_unsupported_gles_cap(cap) {
        log::debug!(
            "[FluorateGL] glEnable(0x{:04X}) ignored (unsupported in GLES)",
            cap
        );
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.enable)(cap);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDisable(cap: u32) {
    if is_unsupported_gles_cap(cap) {
        log::debug!(
            "[FluorateGL] glDisable(0x{:04X}) ignored (unsupported in GLES)",
            cap
        );
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.disable)(cap);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDepthFunc(func: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.depth_func)(func);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDepthMask(flag: u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.depth_mask)(flag);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendFunc(sfactor: u32, dfactor: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_func)(sfactor, dfactor);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearColor(r: f32, g: f32, b: f32, a: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_color)(r, g, b, a);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearDepth(depth: f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_depth)(depth as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearStencil(s: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_stencil)(s);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glViewport(x: i32, y: i32, width: i32, height: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.viewport)(x, y, width, height);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glScissor(x: i32, y: i32, width: i32, height: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.scissor)(x, y, width, height);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCullFace(mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.cull_face)(mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFrontFace(mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.front_face)(mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glLineWidth(width: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.line_width)(width);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glActiveTexture(texture: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.active_texture)(texture);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPixelStorei(pname: u32, param: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.pixel_store_i)(pname, param);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArrays(mode: u32, first: i32, count: i32) {
    // 同步持久映射 buffer 脏区域（若 vertex buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    let bound_vao = state::with_state(|s| s.bound_vertex_array);
    let bound_buf = state::with_state(|s| s.bound_buffer);
    log::debug!(
        "[FluorateGL] glDrawArrays(mode=0x{:04X}, first={}, count={}) bound_vao={} bound_buf={} (tid={})",
        mode,
        first,
        count,
        bound_vao,
        bound_buf,
        state::thread_id_u64()
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_arrays)(mode, first, count);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElements(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
) {
    // 同步持久映射 buffer 脏区域（若 vertex/index buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    let bound_vao = state::with_state(|s| s.bound_vertex_array);
    let bound_buf = state::with_state(|s| s.bound_buffer);
    log::debug!(
        "[FluorateGL] glDrawElements(mode=0x{:04X}, count={}, type=0x{:04X}, indices={:?}) bound_vao={} bound_buf={} (tid={})",
        mode,
        count,
        type_,
        indices,
        bound_vao,
        bound_buf,
        state::thread_id_u64()
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_elements)(mode, count, type_, indices);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFinish() {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.finish)();
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFlush() {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.flush)();
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenerateMipmap(target: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.generate_mipmap)(target);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetError() -> u32 {
    let err = backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.get_error)() });
    if err != 0 {
        // MC blaze3d 每帧轮询 glGetError，驱动偶发 GL_INVALID_ENUM 会刷屏，降为 debug。
        // 真正的错误（如 shader 编译失败）会通过 fail-fast error 日志体现。
        log::debug!("[FluorateGL] glGetError() -> 0x{:04X} (GL error)", err);
    }
    err
}

fn get_fake_extensions_string() -> *const c_char {
    static EXT_STRING: OnceLock<CString> = OnceLock::new();
    let s = EXT_STRING.get_or_init(|| {
        let joined = FAKE_EXTENSIONS
            .iter()
            .map(|ext| std::str::from_utf8(&ext[..ext.len() - 1]).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        CString::new(joined).unwrap_or_else(|_| CString::new("").unwrap())
    });
    s.as_ptr() as *const _
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetString(name: u32) -> *const c_char {
    let result = if name == 0x1F02 {
        // GL_VERSION：从 config::REPORTED_GL_VERSION_PREFIX 拼接 FluorateGL 版本号，
        // MC F3 的 "OpenGL:" 行显示如 "3.3.0 FluorateGL v0.2.0"。
        // 版本前缀在 config.rs 统一维护，此处不再硬编码，避免两处定义不同步。
        static VERSION: OnceLock<CString> = OnceLock::new();
        let v = VERSION.get_or_init(|| {
            CString::new(format!(
                "{} v{}",
                crate::config::REPORTED_GL_VERSION_PREFIX,
                env!("CARGO_PKG_VERSION")
            ))
            .unwrap_or_else(|_| CString::new("").unwrap())
        });
        v.as_ptr() as *const c_char
    } else if name == 0x8B8C {
        // GL_SHADING_LANGUAGE_VERSION
        static GLSL: OnceLock<CString> = OnceLock::new();
        let s = GLSL.get_or_init(|| CString::new(crate::config::REPORTED_GLSL_VERSION).unwrap());
        s.as_ptr() as *const c_char
    } else if name == 0x1F03 {
        // GL_EXTENSIONS
        get_fake_extensions_string()
    } else {
        let raw = backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.get_string)(name) });
        // ANGLE 等后端可能在上下文未完全初始化时返回 null，兜底返回 "Unknown"
        // 避免 GpuDevice.getRenderer() → NullPointerException
        if raw.is_null() {
            match name {
                0x1F01 => {
                    static UNKNOWN_RENDERER: &[u8] = b"Unknown\0";
                    UNKNOWN_RENDERER.as_ptr() as *const c_char
                }
                0x1F00 => {
                    static UNKNOWN_VENDOR: &[u8] = b"Unknown\0";
                    UNKNOWN_VENDOR.as_ptr() as *const c_char
                }
                _ => raw,
            }
        } else {
            raw
        }
    };
    log::debug!("[FluorateGL] glGetString(0x{:04X}) -> {:?}", name, result);
    result
}

static FAKE_EXTENSIONS: &[&[u8]] = &[
    b"GL_ARB_vertex_array_object\0",
    b"GL_ARB_framebuffer_object\0",
    b"GL_ARB_instanced_arrays\0",
    b"GL_ARB_uniform_buffer_object\0",
    b"GL_ARB_shader_storage_buffer_object\0",
    b"GL_ARB_shader_image_load_store\0",
    b"GL_ARB_separate_shader_objects\0",
    b"GL_ARB_vertex_attrib_binding\0",
    // Draw 系列扩展：对应 dispatch.rs 中已加载的扩展函数。
    // 声明必须与 capabilities 检测结果一致，否则宿主查询扩展后调用 stub 函数会崩溃。
    // GLES 3.1 core：glDrawArraysIndirect / glDrawElementsIndirect
    b"GL_ARB_draw_indirect\0",
    // GLES 3.1 core：glMultiDrawArrays / glMultiDrawElements（部分驱动以 stub 加载）
    // 桌面对应 GL_ARB_multi_draw_indirect，Sodium 0.8+ 查询此扩展决定是否启用 chunk batching
    b"GL_ARB_multi_draw_indirect\0",
    b"GL_EXT_multi_draw_indirect\0",
    // GLES 3.2 / GL_OES_draw_elements_base_vertex：glDrawElementsBaseVertex 系列
    b"GL_ARB_draw_elements_base_vertex\0",
    b"GL_OES_draw_elements_base_vertex\0",
    // GLES 3.2 / GL_EXT_base_instance：glDrawArraysInstancedBaseInstance 系列
    b"GL_ARB_base_instance\0",
    b"GL_EXT_base_instance\0",
    // GLES 3.2 / GL_EXT_multi_draw_elements_base_vertex：glMultiDrawElementsBaseVertex
    b"GL_EXT_multi_draw_elements_base_vertex\0",
    b"GL_ARB_timer_query\0",
    b"GL_ARB_buffer_storage\0",
    b"GL_ARB_get_program_binary\0",
    b"GL_ARB_clear_texture\0",
    b"GL_ARB_draw_buffers_blend\0",
    b"GL_ARB_depth_texture\0",
    b"GL_ARB_ES3_compatibility\0",
    b"GL_ARB_shading_language_100\0",
    b"GL_ARB_texture_storage\0",
    b"GL_ARB_texture_float\0",
    b"GL_ARB_texture_half_float\0",
    b"GL_ARB_texture_rg\0",
    b"GL_ARB_texture_compression_bptc\0",
    b"GL_ARB_texture_compression_rgtc\0",
    b"GL_EXT_texture_compression_s3tc\0",
    b"GL_EXT_texture_filter_anisotropic\0",
    b"GL_EXT_texture_sRGB\0",
    b"GL_EXT_color_buffer_float\0",
    b"GL_EXT_disjoint_timer_query\0",
    b"GL_KHR_debug\0",
    b"GL_KHR_no_error\0",
    b"GL_KHR_texture_compression_astc_ldr\0",
    b"GL_KHR_texture_compression_astc_hdr\0",
    b"GL_OES_texture_float\0",
    b"GL_OES_texture_half_float\0",
    b"GL_OES_texture_half_float_linear\0",
    b"GL_EXT_geometry_shader\0",
    b"GL_EXT_tessellation_shader\0",
    b"GL_EXT_texture_cube_map_array\0",
    b"GL_EXT_gpu_shader5\0",
    b"GL_EXT_draw_buffers_indexed\0",
    b"GL_EXT_copy_image\0",
    b"GL_EXT_texture_border_clamp\0",
    b"GL_EXT_texture_buffer\0",
    b"GL_EXT_shader_framebuffer_fetch\0",
    b"GL_OES_standard_derivatives\0",
    b"GL_OES_element_index_uint\0",
    b"GL_OES_texture_npot\0",
    b"GL_OES_depth_texture\0",
    b"GL_OES_packed_depth_stencil\0",
    b"GL_OES_rgb8_rgba8\0",
];

/// 将 GLES 驱动返回的绑定查询结果中的原始 GLES ID 翻译为桌面 ID。
/// 如果 pname 不是绑定查询，或返回值为 0，则不做任何修改。
fn translate_binding_to_desktop(pname: u32, data: *mut i32) {
    let gles_id = unsafe { *data } as u32;
    if gles_id == 0 {
        return;
    }

    let desktop_id = match pname {
        // Buffer 绑定查询 → buffers IdMap
        0x8894 | // GL_ARRAY_BUFFER_BINDING
        0x8895 | // GL_ELEMENT_ARRAY_BUFFER_BINDING
        0x8A28 | // GL_UNIFORM_BUFFER_BINDING
        0x8F36 | // GL_COPY_READ_BUFFER_BINDING
        0x8F37 | // GL_COPY_WRITE_BUFFER_BINDING
        0x8C8F | // GL_TRANSFORM_FEEDBACK_BUFFER_BINDING
        0x88ED | // GL_PIXEL_PACK_BUFFER_BINDING
        0x88EF | // GL_PIXEL_UNPACK_BUFFER_BINDING
        0x8F43 | // GL_DRAW_INDIRECT_BUFFER_BINDING
        0x90D3 // GL_SHADER_STORAGE_BUFFER_BINDING
        => {
            state::with_state(|s| s.buffers.get_desktop(gles_id))
        }
        // Vertex Array 绑定 → vertex_arrays IdMap
        0x85B5 /* GL_VERTEX_ARRAY_BINDING */ => {
            state::with_state(|s| s.vertex_arrays.get_desktop(gles_id))
        }
        // Program 绑定 → programs IdMap
        0x8B8D /* GL_CURRENT_PROGRAM */ => {
            state::with_state(|s| s.programs.get_desktop(gles_id))
        }
        // Texture 绑定 → textures IdMap
        0x8069 | // GL_TEXTURE_BINDING_2D
        0x806A | // GL_TEXTURE_BINDING_3D
        0x8C1D | // GL_TEXTURE_BINDING_2D_ARRAY
        0x8514 // GL_TEXTURE_BINDING_CUBE_MAP
        => {
            state::with_state(|s| s.textures.get_desktop(gles_id))
        }
        // Framebuffer 绑定 → framebuffers IdMap
        0x8CA6 | // GL_DRAW_FRAMEBUFFER_BINDING (= GL_FRAMEBUFFER_BINDING)
        0x8CAA // GL_READ_FRAMEBUFFER_BINDING
        => {
            state::with_state(|s| s.framebuffers.get_desktop(gles_id))
        }
        // Renderbuffer 绑定 → renderbuffers IdMap
        0x8CA7 /* GL_RENDERBUFFER_BINDING */ => {
            state::with_state(|s| s.renderbuffers.get_desktop(gles_id))
        }
        _ => return, // 不是绑定查询，无需翻译
    };

    if let Some(desktop_id) = desktop_id {
        if desktop_id != gles_id {
            unsafe { *data = desktop_id as i32 };
        }
    } else {
        warn_binding_id_miss(pname, gles_id);
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetIntegerv(pname: u32, data: *mut i32) {
    if data.is_null() {
        return;
    }
    match pname {
        0x821D => {
            // GL_NUM_EXTENSIONS
            unsafe { *data = FAKE_EXTENSIONS.len() as i32 };
        }
        0x821B => {
            // GL_MAJOR_VERSION
            unsafe { *data = crate::config::REPORTED_GL_MAJOR };
        }
        0x821C => {
            // GL_MINOR_VERSION
            unsafe { *data = crate::config::REPORTED_GL_MINOR };
        }
        0x9126 => {
            // GL_CONTEXT_PROFILE_MASK
            unsafe { *data = 0x00000001 }; // GL_CONTEXT_CORE_PROFILE_BIT
        }
        _ => {
            getter::get_integerv(pname, data);
            translate_binding_to_desktop(pname, data);
        }
    }
    log::debug!(
        "[FluorateGL] glGetIntegerv(0x{:04X}) -> {}",
        pname,
        unsafe { *data }
    );
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetStringi(name: u32, index: u32) -> *const c_char {
    let result = if name == 0x1F03 && (index as usize) < FAKE_EXTENSIONS.len() {
        FAKE_EXTENSIONS[index as usize].as_ptr() as *const c_char
    } else {
        std::ptr::null()
    };
    log::debug!(
        "[FluorateGL] glGetStringi(0x{:04X}, {}) -> {:?}",
        name,
        index,
        result
    );
    result
}
