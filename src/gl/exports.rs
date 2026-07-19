use crate::backend;
use crate::gl::getter;
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

// Capabilities that exist in desktop GL but are unsupported (or always on)
// in OpenGL ES. Passing them to GLES produces `GL_INVALID_ENUM`.
fn is_unsupported_gles_cap(cap: u32) -> bool {
    matches!(
        cap,
        0x884F | // GL_TEXTURE_CUBE_MAP_SEAMLESS
        0x8642 | // GL_PROGRAM_POINT_SIZE
        0x0B10 | // GL_POINT_SMOOTH
        0x0B20 | // GL_LINE_SMOOTH
        0x0B41 | // GL_POLYGON_SMOOTH
        0x809D | // GL_MULTISAMPLE
        0x0B21 // GL_LINE_STIPPLE
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
        log::debug!("[FluorateGL] glGetError() -> 0x{:04X}", err);
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
        backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.get_string)(name) })
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
        _ => getter::get_integerv(pname, data),
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
