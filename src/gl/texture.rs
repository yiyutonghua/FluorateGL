use crate::backend;
use crate::state;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenTextures(n: i32, textures: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let mut gles_id = 0u32;
            (dispatch.gen_textures)(1, &mut gles_id);
            
            let desktop_id = state::with_state(|s| s.textures.alloc(gles_id));
            *textures.offset(i) = desktop_id;
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteTextures(n: i32, textures: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let desktop_id = *textures.offset(i);
            if let Some(gles_id) = state::with_state(|s| s.textures.delete(desktop_id)) {
                (dispatch.delete_textures)(1, &gles_id);
            }
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindTexture(target: u32, texture: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if texture == 0 {
            0
        } else {
            state::with_state(|s| s.textures.get_gles(texture).unwrap_or(0))
        };
        
        (dispatch.bind_texture)(target, gles_id);
        state::with_state(|s| s.bound_texture = texture);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexImage2D(
    target: u32,
    level: i32,
    internalformat: i32,
    width: i32,
    height: i32,
    border: i32,
    format: u32,
    type_: u32,
    pixels: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_image_2d)(target, level, internalformat, width, height, border, format, type_, pixels);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexSubImage2D(
    target: u32,
    level: i32,
    xoffset: i32,
    yoffset: i32,
    width: i32,
    height: i32,
    format: u32,
    type_: u32,
    pixels: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_sub_image_2d)(target, level, xoffset, yoffset, width, height, format, type_, pixels);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameteri(target: u32, pname: u32, param: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_parameter_i)(target, pname, param);
    });
}
