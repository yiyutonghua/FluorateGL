use std::ffi::c_void;

pub struct GlesDispatch {
    // A类
    pub clear: unsafe extern "C" fn(u32),
    pub get_string: unsafe extern "C" fn(u32) -> *const i8,
    pub enable: unsafe extern "C" fn(u32),
    pub disable: unsafe extern "C" fn(u32),
    pub depth_func: unsafe extern "C" fn(u32),
    pub depth_mask: unsafe extern "C" fn(u8),
    pub blend_func: unsafe extern "C" fn(u32, u32),
    pub clear_color: unsafe extern "C" fn(f32, f32, f32, f32),
    pub clear_depth: unsafe extern "C" fn(f32),
    pub clear_stencil: unsafe extern "C" fn(i32),
    pub viewport: unsafe extern "C" fn(i32, i32, i32, i32),
    pub scissor: unsafe extern "C" fn(i32, i32, i32, i32),
    pub cull_face: unsafe extern "C" fn(u32),
    pub front_face: unsafe extern "C" fn(u32),
    pub line_width: unsafe extern "C" fn(f32),
    pub active_texture: unsafe extern "C" fn(u32),
    pub pixel_store_i: unsafe extern "C" fn(u32, i32),
    pub draw_arrays: unsafe extern "C" fn(u32, i32, i32),
    pub draw_elements: unsafe extern "C" fn(u32, i32, u32, *const c_void),
    pub finish: unsafe extern "C" fn(),
    pub flush: unsafe extern "C" fn(),
    pub generate_mipmap: unsafe extern "C" fn(u32),
    pub get_error: unsafe extern "C" fn() -> u32,

    // B类：Buffer
    pub gen_buffers: unsafe extern "C" fn(i32, *mut u32),
    pub delete_buffers: unsafe extern "C" fn(i32, *const u32),
    pub bind_buffer: unsafe extern "C" fn(u32, u32),
    pub buffer_data: unsafe extern "C" fn(u32, isize, *const c_void, u32),
    pub buffer_sub_data: unsafe extern "C" fn(u32, isize, isize, *const c_void),

    // B类：VAO
    pub gen_vertex_arrays: unsafe extern "C" fn(i32, *mut u32),
    pub delete_vertex_arrays: unsafe extern "C" fn(i32, *const u32),
    pub bind_vertex_array: unsafe extern "C" fn(u32),
    pub enable_vertex_attrib_array: unsafe extern "C" fn(u32),
    pub disable_vertex_attrib_array: unsafe extern "C" fn(u32),
    pub vertex_attrib_pointer: unsafe extern "C" fn(u32, i32, u32, u8, i32, *const c_void),
    pub vertex_attrib_i_pointer: unsafe extern "C" fn(u32, i32, u32, i32, *const c_void),

    // B类：Shader
    pub create_shader: unsafe extern "C" fn(u32) -> u32,
    pub delete_shader: unsafe extern "C" fn(u32),
    pub shader_source: unsafe extern "C" fn(u32, i32, *const *const i8, *const i32),
    pub compile_shader: unsafe extern "C" fn(u32),
    pub get_shader_iv: unsafe extern "C" fn(u32, u32, *mut i32),
    pub get_shader_info_log: unsafe extern "C" fn(u32, i32, *mut i32, *mut i8),

    // B类：Program
    pub create_program: unsafe extern "C" fn() -> u32,
    pub delete_program: unsafe extern "C" fn(u32),
    pub attach_shader: unsafe extern "C" fn(u32, u32),
    pub link_program: unsafe extern "C" fn(u32),
    pub use_program: unsafe extern "C" fn(u32),
    pub get_program_iv: unsafe extern "C" fn(u32, u32, *mut i32),
    pub get_program_info_log: unsafe extern "C" fn(u32, i32, *mut i32, *mut i8),
    pub get_uniform_location: unsafe extern "C" fn(u32, *const i8) -> i32,
    pub get_attrib_location: unsafe extern "C" fn(u32, *const i8) -> i32,
    pub uniform_1f: unsafe extern "C" fn(i32, f32),
    pub uniform_1i: unsafe extern "C" fn(i32, i32),
    pub uniform_matrix_4fv: unsafe extern "C" fn(i32, i32, u8, *const f32),

    // B类：Texture
    pub gen_textures: unsafe extern "C" fn(i32, *mut u32),
    pub delete_textures: unsafe extern "C" fn(i32, *const u32),
    pub bind_texture: unsafe extern "C" fn(u32, u32),
    pub tex_image_2d: unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void),
    pub tex_sub_image_2d: unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void),
    pub tex_parameter_i: unsafe extern "C" fn(u32, u32, i32),

    // B类：其他
    pub get_integerv: unsafe extern "C" fn(u32, *mut i32),
    pub get_string_i: unsafe extern "C" fn(u32, u32) -> *const i8,

}

impl GlesDispatch {
    pub fn load_from(loader: &super::loader::GlesLoader) -> Option<Self> {
        macro_rules! load {
            ($name:expr) => {{
                let ptr = loader.get_proc($name);
                if ptr.is_null() {
                    return None;
                }
                ptr
            }};
        }

        Some(Self {
            clear: unsafe { std::mem::transmute(load!("glClear")) },
            get_string: unsafe { std::mem::transmute(load!("glGetString")) },
            enable: unsafe { std::mem::transmute(load!("glEnable")) },
            disable: unsafe { std::mem::transmute(load!("glDisable")) },
            depth_func: unsafe { std::mem::transmute(load!("glDepthFunc")) },
            depth_mask: unsafe { std::mem::transmute(load!("glDepthMask")) },
            blend_func: unsafe { std::mem::transmute(load!("glBlendFunc")) },
            clear_color: unsafe { std::mem::transmute(load!("glClearColor")) },
            clear_depth: unsafe { std::mem::transmute(load!("glClearDepthf")) },
            clear_stencil: unsafe { std::mem::transmute(load!("glClearStencil")) },
            viewport: unsafe { std::mem::transmute(load!("glViewport")) },
            scissor: unsafe { std::mem::transmute(load!("glScissor")) },
            cull_face: unsafe { std::mem::transmute(load!("glCullFace")) },
            front_face: unsafe { std::mem::transmute(load!("glFrontFace")) },
            line_width: unsafe { std::mem::transmute(load!("glLineWidth")) },
            active_texture: unsafe { std::mem::transmute(load!("glActiveTexture")) },
            pixel_store_i: unsafe { std::mem::transmute(load!("glPixelStorei")) },
            draw_arrays: unsafe { std::mem::transmute(load!("glDrawArrays")) },
            draw_elements: unsafe { std::mem::transmute(load!("glDrawElements")) },
            finish: unsafe { std::mem::transmute(load!("glFinish")) },
            flush: unsafe { std::mem::transmute(load!("glFlush")) },
            generate_mipmap: unsafe { std::mem::transmute(load!("glGenerateMipmap")) },
            get_error: unsafe { std::mem::transmute(load!("glGetError")) },

            gen_buffers: unsafe { std::mem::transmute(load!("glGenBuffers")) },
            delete_buffers: unsafe { std::mem::transmute(load!("glDeleteBuffers")) },
            bind_buffer: unsafe { std::mem::transmute(load!("glBindBuffer")) },
            buffer_data: unsafe { std::mem::transmute(load!("glBufferData")) },
            buffer_sub_data: unsafe { std::mem::transmute(load!("glBufferSubData")) },

            gen_vertex_arrays: unsafe { std::mem::transmute(load!("glGenVertexArrays")) },
            delete_vertex_arrays: unsafe { std::mem::transmute(load!("glDeleteVertexArrays")) },
            bind_vertex_array: unsafe { std::mem::transmute(load!("glBindVertexArray")) },
            enable_vertex_attrib_array: unsafe { std::mem::transmute(load!("glEnableVertexAttribArray")) },
            disable_vertex_attrib_array: unsafe { std::mem::transmute(load!("glDisableVertexAttribArray")) },
            vertex_attrib_pointer: unsafe { std::mem::transmute(load!("glVertexAttribPointer")) },
            vertex_attrib_i_pointer: unsafe { std::mem::transmute(load!("glVertexAttribIPointer")) },

            create_shader: unsafe { std::mem::transmute(load!("glCreateShader")) },
            delete_shader: unsafe { std::mem::transmute(load!("glDeleteShader")) },
            shader_source: unsafe { std::mem::transmute(load!("glShaderSource")) },
            compile_shader: unsafe { std::mem::transmute(load!("glCompileShader")) },
            get_shader_iv: unsafe { std::mem::transmute(load!("glGetShaderiv")) },
            get_shader_info_log: unsafe { std::mem::transmute(load!("glGetShaderInfoLog")) },

            create_program: unsafe { std::mem::transmute(load!("glCreateProgram")) },
            delete_program: unsafe { std::mem::transmute(load!("glDeleteProgram")) },
            attach_shader: unsafe { std::mem::transmute(load!("glAttachShader")) },
            link_program: unsafe { std::mem::transmute(load!("glLinkProgram")) },
            use_program: unsafe { std::mem::transmute(load!("glUseProgram")) },
            get_program_iv: unsafe { std::mem::transmute(load!("glGetProgramiv")) },
            get_program_info_log: unsafe { std::mem::transmute(load!("glGetProgramInfoLog")) },
            get_uniform_location: unsafe { std::mem::transmute(load!("glGetUniformLocation")) },
            get_attrib_location: unsafe { std::mem::transmute(load!("glGetAttribLocation")) },
            uniform_1f: unsafe { std::mem::transmute(load!("glUniform1f")) },
            uniform_1i: unsafe { std::mem::transmute(load!("glUniform1i")) },
            uniform_matrix_4fv: unsafe { std::mem::transmute(load!("glUniformMatrix4fv")) },

            gen_textures: unsafe { std::mem::transmute(load!("glGenTextures")) },
            delete_textures: unsafe { std::mem::transmute(load!("glDeleteTextures")) },
            bind_texture: unsafe { std::mem::transmute(load!("glBindTexture")) },
            tex_image_2d: unsafe { std::mem::transmute(load!("glTexImage2D")) },
            tex_sub_image_2d: unsafe { std::mem::transmute(load!("glTexSubImage2D")) },
            tex_parameter_i: unsafe { std::mem::transmute(load!("glTexParameteri")) },

            get_integerv: unsafe { std::mem::transmute(load!("glGetIntegerv")) },
            get_string_i: unsafe { std::mem::transmute(load!("glGetStringi")) },

        })
    }
}

