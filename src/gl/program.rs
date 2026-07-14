use crate::backend;
use crate::state;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCreateProgram() -> u32 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = (dispatch.create_program)();
        state::with_state(|s| s.programs.alloc(gles_id))
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteProgram(program: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if let Some(gles_id) = state::with_state(|s| s.programs.delete(program)) {
            (dispatch.delete_program)(gles_id);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glAttachShader(program: u32, shader: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_program = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        let gles_shader = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_program == 0 || gles_shader == 0 {
            return;
        }
        (dispatch.attach_shader)(gles_program, gles_shader);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glLinkProgram(program: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.link_program)(gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUseProgram(program: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if program == 0 {
            0
        } else {
            state::with_state(|s| s.programs.get_gles(program).unwrap_or(0))
        };
        (dispatch.use_program)(gles_id);
        state::with_state(|s| s.bound_program = program);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetProgramiv(program: u32, pname: u32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_program_iv)(gles_id, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetProgramInfoLog(
    program: u32,
    buf_size: i32,
    length: *mut i32,
    info_log: *mut i8,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_program_info_log)(gles_id, buf_size, length, info_log);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformLocation(program: u32, name: *const i8) -> i32 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return -1;
        }
        (dispatch.get_uniform_location)(gles_id, name)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetAttribLocation(program: u32, name: *const i8) -> i32 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return -1;
        }
        (dispatch.get_attrib_location)(gles_id, name)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform1f(location: i32, v0: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_1f)(location, v0);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform1i(location: i32, v0: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_1i)(location, v0);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniformMatrix4fv(
    location: i32,
    count: i32,
    transpose: u8,
    value: *const f32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_matrix_4fv)(location, count, transpose, value);
    });
}
