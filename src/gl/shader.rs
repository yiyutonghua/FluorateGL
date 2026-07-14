use crate::backend;
use crate::state;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCreateShader(shader_type: u32) -> u32 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = (dispatch.create_shader)(shader_type);
        state::with_state(|s| s.shaders.alloc(gles_id))
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteShader(shader: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
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
    string: *const *const i8,
    length: *const i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        
        // TODO: GLSL 转译（Phase 2）
        // 现在直接透传
        (dispatch.shader_source)(gles_id, count, string, length);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCompileShader(shader: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.compile_shader)(gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetShaderiv(shader: u32, pname: u32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_id == 0 {
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
    info_log: *mut i8,
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
