use crate::backend;
use crate::gl::getter;
use crate::state;
use libc::c_char;
use std::ffi::CString;
use std::sync::OnceLock;

// === A类：直接透传 ===

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClear(mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear)(mask);
    });
}

// glAlphaFunc 是桌面 GL 固定功能，GLES 2.0+ 不支持，直接忽略
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glAlphaFunc(_func: u32, _ref: f32) {
    // no-op: GLES 不支持固定功能 alpha test，alpha test 在 shader 中通过 discard 实现
}

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
        0x0BC0 // GL_ALPHA_TEST (GLES 2.0+ 不支持，alpha test 在 shader 中通过 discard 实现)
    )
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEnable(cap: u32) {
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
        log::warn!("[FluorateGL] glGetError() -> 0x{:04X} (GL error)", err);
    }
    err
}

// === 特殊处理 ===

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
        // GL_VERSION
        static VERSION: &[u8] = b"3.2.0 FluorateGL\0";
        VERSION.as_ptr() as *const c_char
    } else if name == 0x8B8C {
        // GL_SHADING_LANGUAGE_VERSION
        static GLSL: &[u8] = b"3.30\0";
        GLSL.as_ptr() as *const c_char
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
        log::warn!(
            "[FluorateGL] glGetIntegerv(0x{:04X}): GLES ID {} not found in IdMap, returning raw GLES ID",
            pname,
            gles_id
        );
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
            unsafe { *data = 3 };
        }
        0x821C => {
            // GL_MINOR_VERSION
            unsafe { *data = 2 };
        }
        0x9126 => {
            // GL_CONTEXT_PROFILE_MASK
            unsafe { *data = 0x00000001 }; // GL_CONTEXT_CORE_PROFILE_BIT
        }
        _ => {
            getter::get_integerv(pname, data);
            // 将 GLES 驱动返回的原始 GLES ID 翻译为桌面 ID
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
