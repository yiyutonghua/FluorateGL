use crate::backend;

// === A类：直接透传 ===

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClear(mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear)(mask);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEnable(cap: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.enable)(cap);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDisable(cap: u32) {
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
pub extern "C" fn glDrawElements(mode: u32, count: i32, type_: u32, indices: *const std::ffi::c_void) {
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_error)()
    })
}

// === 特殊处理 ===

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetString(name: u32) -> *const i8 {
    if name == 0x1F02 {
        static VERSION: &[u8] = b"3.2.0 FluorateGL\0";
        return VERSION.as_ptr() as *const i8;
    }
    if name == 0x1F03 {
        static GLSL: &[u8] = b"3.30\0";
        return GLSL.as_ptr() as *const i8;
    }
    
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_string)(name)
    })
}

static FAKE_EXTENSIONS: &[&[u8]] = &[
    b"GL_ARB_vertex_array_object\0",
    b"GL_ARB_framebuffer_object\0",
    b"GL_ARB_instanced_arrays\0",
    b"GL_ARB_uniform_buffer_object\0",
];

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetIntegerv(pname: u32, data: *mut i32) {
    if pname == 0x821D { // GL_NUM_EXTENSIONS
        unsafe { *data = FAKE_EXTENSIONS.len() as i32 };
        return;
    }
    
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_integerv)(pname, data);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetStringi(name: u32, index: u32) -> *const i8 {
    if name == 0x1F03 && (index as usize) < FAKE_EXTENSIONS.len() {
        return FAKE_EXTENSIONS[index as usize].as_ptr() as *const i8;
    }
    
    // 超出范围的索引返回空指针
    std::ptr::null()
}
