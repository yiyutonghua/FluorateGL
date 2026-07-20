use crate::backend;
use crate::state;

// Texture internal format mappings: desktop OpenGL -> GLES 3.x
// GLES 3.x supports sized internal formats, but some legacy desktop-only
// formats (e.g. GL_RGB16, GL_DEPTH_COMPONENT32) need to be emulated.
// ================= 补充完整的常量映射 =================
const GL_RED: u32 = 0x1903;
const GL_ALPHA: u32 = 0x1906;
const GL_RGB: u32 = 0x1907;
const GL_RGBA: u32 = 0x1908;
const GL_LUMINANCE: u32 = 0x1909;
const GL_LUMINANCE_ALPHA: u32 = 0x190A;
const GL_RG: u32 = 0x8227;
const GL_DEPTH_COMPONENT: u32 = 0x1902;
const GL_DEPTH_STENCIL: u32 = 0x84F9;

const GL_R8: u32 = 0x8229;
const GL_RG8: u32 = 0x822B;
const GL_RGB8: u32 = 0x8051;
const GL_RGBA8: u32 = 0x8058;
const GL_DEPTH_COMPONENT16: u32 = 0x81A5;
const GL_DEPTH_COMPONENT24: u32 = 0x81A6;
const GL_DEPTH24_STENCIL8: u32 = 0x88F0;

// 你原有的常量保留...
const GL_R3_G3_B2: u32 = 0x2A10;
const GL_RGB4: u32 = 0x804F;
const GL_RGB5: u32 = 0x8050;
const GL_RGB10: u32 = 0x8052;
const GL_RGB12: u32 = 0x8053;
const GL_RGB16: u32 = 0x8054;
const GL_RGBA2: u32 = 0x8055;
const GL_RGBA4: u32 = 0x8056;
const GL_RGB10_A2: u32 = 0x8059;
const GL_RGBA12: u32 = 0x805A;
const GL_RGBA16: u32 = 0x805B;
const GL_BGR: u32 = 0x80E0;
const GL_BGRA: u32 = 0x80E1;
const GL_DEPTH_COMPONENT32: u32 = 0x81A7;
const GL_DEPTH_COMPONENT32F: u32 = 0x8CAC;
const GL_STENCIL_INDEX8: u32 = 0x8D48;
const GL_STENCIL_INDEX16: u32 = 0x8D49;
const GL_COMPRESSED_RGBA: u32 = 0x84EE;
const GL_COMPRESSED_RGB: u32 = 0x84ED;

/// Convert a desktop OpenGL internal format to the closest GLES-compatible
/// internal format.
fn normalize_internal_format(internalformat: u32) -> u32 {
    match internalformat {
        // 1. 最常见的 Unsized Formats (MC 常用，GLES3 的 glTexStorage 必须转换)
        GL_RED | GL_ALPHA | GL_LUMINANCE => GL_R8,
        GL_RG | GL_LUMINANCE_ALPHA => GL_RG8,
        GL_RGB => GL_RGB8,
        GL_RGBA => GL_RGBA8,

        // 2. 深度/模板缓冲格式
        GL_DEPTH_COMPONENT => GL_DEPTH_COMPONENT24,
        GL_DEPTH_STENCIL => GL_DEPTH24_STENCIL8,

        // 3. 你原有的 Legacy Desktop 格式映射
        GL_R3_G3_B2 | GL_RGB4 | GL_RGB5 | GL_RGB12 => GL_RGB8,
        GL_RGB10 => GL_RGB10_A2,
        GL_RGB16 => GL_RGBA16,
        GL_RGBA2 | GL_RGBA4 | GL_RGBA12 => GL_RGBA8,
        GL_BGR => GL_RGB8,
        GL_BGRA => GL_RGBA8,
        GL_DEPTH_COMPONENT32 => GL_DEPTH_COMPONENT32F,
        GL_STENCIL_INDEX8 | GL_STENCIL_INDEX16 => GL_R8,

        // 4. 桌面 GL 的"让驱动自选压缩格式"标志，GLES 不支持，降级为 sized 格式
        GL_COMPRESSED_RGBA => GL_RGBA8,
        GL_COMPRESSED_RGB => GL_RGB8,

        // 5. 其他情况（已经是 Sized Format）原样返回
        _ => internalformat,
    }
}

/// 判断 internalformat 是否为已知的压缩纹理格式。
/// 用于 `glCompressedTexImage*` 系列函数，防止将非压缩格式透传给 GLES 驱动导致 GL_INVALID_ENUM。
fn is_compressed_format(internalformat: u32) -> bool {
    matches!(
        internalformat,
        // S3TC / DXT
        0x83F0 | 0x83F1 | 0x83F2 | 0x83F3
        // ETC2 / EAC
        | 0x9274 | 0x9275 | 0x9276 | 0x9277 | 0x9278
        // ASTC LDR (4x4 ~ 12x12)
        | 0x93B0 | 0x93B1 | 0x93B2 | 0x93B3
        | 0x93B4 | 0x93B5 | 0x93B6 | 0x93B7
        | 0x93B8 | 0x93B9 | 0x93BA | 0x93BB
        | 0x93BC | 0x93BD | 0x93BE | 0x93BF
        | 0x93C0 | 0x93C1 | 0x93C2 | 0x93C3
        | 0x93C4 | 0x93C5 | 0x93C6 | 0x93C7
        | 0x93C8 | 0x93C9 | 0x93CA | 0x93CB
        | 0x93CC | 0x93CD | 0x93CE | 0x93CF
        | 0x93D0 | 0x93D1 | 0x93D2 | 0x93D3
        | 0x93D4 | 0x93D5 | 0x93D6
        // PVRTC
        | 0x8C00 | 0x8C01 | 0x8C02 | 0x8C03 | 0x8C04
    )
}

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
    let normalized = normalize_internal_format(internalformat as u32) as i32;
    log::info!(
        "[FluorateGL] glTexImage2D(target=0x{:04X}, level={}, internalformat=0x{:04X}, {}x{}, format=0x{:04X}, type=0x{:04X}, pixels={:?})",
        target, level, internalformat, width, height, format, type_, pixels
    );
    if normalized != internalformat {
        log::info!(
            "[FluorateGL] glTexImage2D: normalized internalformat 0x{:04X} -> 0x{:04X}",
            internalformat, normalized
        );
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_image_2d)(
            target, level, normalized, width, height, border, format, type_, pixels,
        );
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
        (dispatch.tex_sub_image_2d)(
            target, level, xoffset, yoffset, width, height, format, type_, pixels,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameteri(target: u32, pname: u32, param: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_parameter_i)(target, pname, param);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexImage3D(
    target: u32,
    level: i32,
    internalformat: i32,
    width: i32,
    height: i32,
    depth: i32,
    border: i32,
    format: u32,
    type_: u32,
    pixels: *const std::ffi::c_void,
) {
    let normalized = normalize_internal_format(internalformat as u32) as i32;
    if normalized != internalformat {
        log::debug!(
            "glTexImage3D: normalized internalformat 0x{:04X} -> 0x{:04X}",
            internalformat,
            normalized
        );
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_image_3d)(
            target, level, normalized, width, height, depth, border, format, type_, pixels,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexSubImage3D(
    target: u32,
    level: i32,
    xoffset: i32,
    yoffset: i32,
    zoffset: i32,
    width: i32,
    height: i32,
    depth: i32,
    format: u32,
    type_: u32,
    pixels: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_sub_image_3d)(
            target, level, xoffset, yoffset, zoffset, width, height, depth, format, type_, pixels,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexStorage2D(
    target: u32,
    levels: i32,
    internalformat: u32,
    width: i32,
    height: i32,
) {
    let normalized = normalize_internal_format(internalformat);
    log::info!(
        "[FluorateGL] glTexStorage2D(target=0x{:04X}, levels={}, internalformat=0x{:04X}, {}x{})",
        target, levels, internalformat, width, height
    );
    if normalized != internalformat {
        log::info!(
            "[FluorateGL] glTexStorage2D: normalized internalformat 0x{:04X} -> 0x{:04X}",
            internalformat, normalized
        );
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_storage_2d)(target, levels, normalized, width, height);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexStorage3D(
    target: u32,
    levels: i32,
    internalformat: u32,
    width: i32,
    height: i32,
    depth: i32,
) {
    let normalized = normalize_internal_format(internalformat);
    if normalized != internalformat {
        log::debug!(
            "glTexStorage3D: normalized internalformat 0x{:04X} -> 0x{:04X}",
            internalformat,
            normalized
        );
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_storage_3d)(target, levels, normalized, width, height, depth);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameterf(target: u32, pname: u32, param: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_parameter_f)(target, pname, param);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameterfv(target: u32, pname: u32, params: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_parameter_fv)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameteriv(target: u32, pname: u32, params: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_parameter_iv)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCompressedTexImage2D(
    target: u32,
    level: i32,
    internalformat: u32,
    width: i32,
    height: i32,
    border: i32,
    imageSize: i32,
    data: *const std::ffi::c_void,
) {
    log::info!(
        "[FluorateGL] glCompressedTexImage2D(target=0x{:04X}, level={}, internalformat=0x{:04X}, {}x{}, imageSize={}, data={:?})",
        target, level, internalformat, width, height, imageSize, data
    );

    // 防止将非压缩格式透传给 GLES 导致 GL_INVALID_ENUM 崩溃
    if !is_compressed_format(internalformat) {
        let normalized = normalize_internal_format(internalformat);
        log::warn!(
            "[FluorateGL] glCompressedTexImage2D: internalformat 0x{:04X} is not a compressed format, normalizing to 0x{:04X} and using glTexImage2D instead",
            internalformat, normalized
        );
        // 对非压缩格式降级为 glTexImage2D（data 指针直接复用，格式兼容）
        backend::with_gles_dispatch(|dispatch| unsafe {
            (dispatch.tex_image_2d)(
                target, level, normalized as i32, width, height, border,
                GL_RGBA, 0x1401, // GL_UNSIGNED_BYTE
                data,
            );
        });
        return;
    }

    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.compressed_tex_image_2d)(
            target,
            level,
            internalformat,
            width,
            height,
            border,
            imageSize,
            data,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCompressedTexSubImage2D(
    target: u32,
    level: i32,
    xoffset: i32,
    yoffset: i32,
    width: i32,
    height: i32,
    format: u32,
    imageSize: i32,
    data: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.compressed_tex_sub_image_2d)(
            target, level, xoffset, yoffset, width, height, format, imageSize, data,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCompressedTexImage3D(
    target: u32,
    level: i32,
    internalformat: u32,
    width: i32,
    height: i32,
    depth: i32,
    border: i32,
    imageSize: i32,
    data: *const std::ffi::c_void,
) {
    log::info!(
        "[FluorateGL] glCompressedTexImage3D(target=0x{:04X}, level={}, internalformat=0x{:04X}, {}x{}x{}, imageSize={}, data={:?})",
        target, level, internalformat, width, height, depth, imageSize, data
    );

    if !is_compressed_format(internalformat) {
        log::warn!(
            "[FluorateGL] glCompressedTexImage3D: internalformat 0x{:04X} is not a compressed format, skipping",
            internalformat
        );
        return;
    }

    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.compressed_tex_image_3d)(
            target,
            level,
            internalformat,
            width,
            height,
            depth,
            border,
            imageSize,
            data,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCompressedTexSubImage3D(
    target: u32,
    level: i32,
    xoffset: i32,
    yoffset: i32,
    zoffset: i32,
    width: i32,
    height: i32,
    depth: i32,
    format: u32,
    imageSize: i32,
    data: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.compressed_tex_sub_image_3d)(
            target, level, xoffset, yoffset, zoffset, width, height, depth, format, imageSize, data,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTexImage(
    target: u32,
    level: i32,
    format: u32,
    type_: u32,
    pixels: *mut std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_tex_image)(target, level, format, type_, pixels);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTexLevelParameteriv(target: u32, level: i32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_tex_level_parameter_iv)(target, level, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTexParameteriv(target: u32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_tex_parameter_iv)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsTexture(texture: u32) -> u8 {
    if texture == 0 {
        return 0;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        // texture 是 desktop id，需要先转回 gles id
        let gles_id = state::with_state(|s| s.textures.get_gles(texture).unwrap_or(0));
        if gles_id == 0 {
            return 0;
        }
        (dispatch.is_texture)(gles_id)
    })
}
