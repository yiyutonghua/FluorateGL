use crate::backend;
use crate::state;
use libc::c_char;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCreateProgram() -> u32 {
    log::debug!("[FluorateGL] glCreateProgram()");
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
    log::debug!("[FluorateGL] glAttachShader({}, {})", program, shader);
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
    log::debug!("[FluorateGL] glLinkProgram({})", program);
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            log::debug!(
                "[FluorateGL] glLinkProgram({}) -> unknown desktop id, skipping",
                program
            );
            return;
        }
        (dispatch.link_program)(gles_id);

        // 检查链接状态
        const GL_LINK_STATUS: u32 = 0x8B82;
        let mut status = 0i32;
        (dispatch.get_program_iv)(gles_id, GL_LINK_STATUS, &mut status);
        if status == 0 {
            let mut len = 0i32;
            const GL_INFO_LOG_LENGTH: u32 = 0x8B84;
            (dispatch.get_program_iv)(gles_id, GL_INFO_LOG_LENGTH, &mut len);
            if len > 0 {
                let mut buf = vec![0u8; len as usize];
                let mut written = 0i32;
                (dispatch.get_program_info_log)(
                    gles_id,
                    len,
                    &mut written,
                    buf.as_mut_ptr() as *mut libc::c_char,
                );
                let info = String::from_utf8_lossy(&buf[..written as usize]);
                log::error!(
                    "[FluorateGL] Program {} (GLES {}) link failed: {}",
                    program,
                    gles_id,
                    info
                );
            } else {
                log::error!(
                    "[FluorateGL] Program {} (GLES {}) link failed (no info log)",
                    program,
                    gles_id
                );
            }
        }
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
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_program_iv)(gles_id, pname, params);

        // fail-fast: 真实返回 link/validate 状态，不欺骗为 GL_TRUE。
        // 保留 error 级诊断日志，让失败有迹可循，便于定位 SPIR-V 翻译根因。
        const GL_LINK_STATUS: u32 = 0x8B82;
        const GL_VALIDATE_STATUS: u32 = 0x8B8B;
        if (pname == GL_LINK_STATUS || pname == GL_VALIDATE_STATUS) && *params == 0 {
            log::error!(
                "[FluorateGL] Program {} (GLES {}) {} failed (fail-fast, returning GL_FALSE)",
                program,
                gles_id,
                if pname == GL_LINK_STATUS {
                    "link"
                } else {
                    "validate"
                }
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetProgramInfoLog(
    program: u32,
    buf_size: i32,
    length: *mut i32,
    info_log: *mut c_char,
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
pub extern "C" fn glGetUniformLocation(program: u32, name: *const c_char) -> i32 {
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
pub extern "C" fn glGetAttribLocation(program: u32, name: *const c_char) -> i32 {
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
pub extern "C" fn glUniformMatrix4fv(location: i32, count: i32, transpose: u8, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_matrix_4fv)(location, count, transpose, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDetachShader(program: u32, shader: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_program = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        let gles_shader = state::with_state(|s| s.shaders.get_gles(shader).unwrap_or(0));
        if gles_program == 0 || gles_shader == 0 {
            return;
        }
        (dispatch.detach_shader)(gles_program, gles_shader);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glValidateProgram(program: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.validate_program)(gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniform(
    program: u32,
    index: u32,
    buf_size: i32,
    length: *mut i32,
    size: *mut i32,
    type_: *mut u32,
    name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_active_uniform)(gles_id, index, buf_size, length, size, type_, name);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveAttrib(
    program: u32,
    index: u32,
    buf_size: i32,
    length: *mut i32,
    size: *mut i32,
    type_: *mut u32,
    name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_active_attrib)(gles_id, index, buf_size, length, size, type_, name);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformfv(program: u32, location: i32, params: *mut f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_uniform_fv)(gles_id, location, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformiv(program: u32, location: i32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_uniform_iv)(gles_id, location, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetAttachedShaders(
    program: u32,
    max_count: i32,
    count: *mut i32,
    shaders: *mut u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }

        if max_count > 0 && !shaders.is_null() {
            let mut gles_shaders = vec![0u32; max_count as usize];
            (dispatch.get_attached_shaders)(gles_id, max_count, count, gles_shaders.as_mut_ptr());

            let returned_count = if count.is_null() {
                max_count
            } else {
                (*count).clamp(0, max_count)
            };

            for i in 0..returned_count as isize {
                let gles_shader = *gles_shaders.as_ptr().offset(i);
                let desktop_shader = state::with_state(|s| {
                    s.shaders.get_desktop(gles_shader).unwrap_or(gles_shader)
                });
                *shaders.offset(i) = desktop_shader;
            }
        } else {
            (dispatch.get_attached_shaders)(gles_id, max_count, count, shaders);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindAttribLocation(program: u32, index: u32, name: *const c_char) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.bind_attrib_location)(gles_id, index, name);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTransformFeedbackVaryings(
    program: u32,
    count: i32,
    varyings: *const *const c_char,
    buffer_mode: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.transform_feedback_varyings)(gles_id, count, varyings, buffer_mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTransformFeedbackVarying(
    program: u32,
    index: u32,
    buf_size: i32,
    length: *mut i32,
    size: *mut i32,
    type_: *mut u32,
    name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_transform_feedback_varying)(
            gles_id, index, buf_size, length, size, type_, name,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniformBlockBinding(
    program: u32,
    uniform_block_index: u32,
    uniform_block_binding: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.uniform_block_binding)(gles_id, uniform_block_index, uniform_block_binding);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformBlockIndex(program: u32, uniform_block_name: *const c_char) -> u32 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return u32::MAX;
        }
        (dispatch.get_uniform_block_index)(gles_id, uniform_block_name)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniformBlockiv(
    program: u32,
    uniform_block_index: u32,
    pname: u32,
    params: *mut i32,
) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_active_uniform_block_iv)(gles_id, uniform_block_index, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniformBlockName(
    program: u32,
    uniform_block_index: u32,
    buf_size: i32,
    length: *mut i32,
    uniform_block_name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_active_uniform_block_name)(
            gles_id,
            uniform_block_index,
            buf_size,
            length,
            uniform_block_name,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformIndices(
    program: u32,
    uniform_count: i32,
    uniform_names: *const *const c_char,
    uniform_indices: *mut u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_uniform_indices)(gles_id, uniform_count, uniform_names, uniform_indices);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniformsiv(
    program: u32,
    uniform_count: i32,
    uniform_indices: *const u32,
    pname: u32,
    params: *mut i32,
) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_active_uniforms_iv)(gles_id, uniform_count, uniform_indices, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsProgram(program: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return 0;
        }
        (dispatch.is_program)(gles_id)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform2f(location: i32, v0: f32, v1: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_2f)(location, v0, v1);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform3f(location: i32, v0: f32, v1: f32, v2: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_3f)(location, v0, v1, v2);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform4f(location: i32, v0: f32, v1: f32, v2: f32, v3: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_4f)(location, v0, v1, v2, v3);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform2i(location: i32, v0: i32, v1: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_2i)(location, v0, v1);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform3i(location: i32, v0: i32, v1: i32, v2: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_3i)(location, v0, v1, v2);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform4i(location: i32, v0: i32, v1: i32, v2: i32, v3: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_4i)(location, v0, v1, v2, v3);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform1fv(location: i32, count: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_1fv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform2fv(location: i32, count: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_2fv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform3fv(location: i32, count: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_3fv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform4fv(location: i32, count: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_4fv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform1iv(location: i32, count: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_1iv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform2iv(location: i32, count: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_2iv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform3iv(location: i32, count: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_3iv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform4iv(location: i32, count: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_4iv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniformMatrix2fv(location: i32, count: i32, transpose: u8, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_matrix_2fv)(location, count, transpose, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniformMatrix3fv(location: i32, count: i32, transpose: u8, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_matrix_3fv)(location, count, transpose, value);
    });
}
