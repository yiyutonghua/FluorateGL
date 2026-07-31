use crate::backend;
use crate::state;

/// 判断 dispatch 函数指针是否为共享的未实现 stub。
///
/// `load_opt!` 把缺失的可选函数替换为同一个 stub 函数，故 GlesDispatch 中所有 stub
/// 字段地址相同。与 `dispatch.stub` 比较即可判定该 GLES 函数是否被驱动支持。
fn is_stub(dispatch: &backend::dispatch::GlesDispatch, ptr: *const ()) -> bool {
    ptr == dispatch.stub as *const ()
}

// Texture internal format mappings: desktop OpenGL -> GLES 3.x
// GLES 3.x supports sized internal formats, but some legacy desktop-only
// formats (e.g. GL_RGB16, GL_DEPTH_COMPONENT32) need to be emulated.
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

// 像素数据类型常量（用于深度格式归一化）
const GL_UNSIGNED_SHORT: u32 = 0x1403;
const GL_UNSIGNED_INT: u32 = 0x1405;
const GL_FLOAT: u32 = 0x1406;
// 桌面 GL 专属纹理参数 pname，GLES 不支持
const GL_TEXTURE_LOD_BIAS: u32 = 0x8501;

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

        // 3. Legacy Desktop 格式映射
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

        _ => internalformat,
    }
}

/// 判断 pname 是否为 GLES 不支持的桌面 GL 纹理参数
///
/// 这些 pname 透传给 GLES 会产生 GL_INVALID_ENUM。MC 旧版（1.21.4 及更早）会调用
/// GL_TEXTURE_LOD_BIAS 设置纹理 LOD 偏移（桌面 GL 固定功能），GLES 无此 pname，
/// LOD 偏移在 GLES 中由 shader 处理。拦截并忽略，避免驱动报错污染错误队列。
fn is_unsupported_tex_parameter(pname: u32) -> bool {
    matches!(pname, GL_TEXTURE_LOD_BIAS)
}

/// 归一化深度/深度模板内部格式，根据像素 type 选择正确的 sized format
///
/// GLES 对深度纹理的 internalformat 与 type 组合有严格要求：
/// - GL_FLOAT(0x1406) 必须配 GL_DEPTH_COMPONENT32F(0x8CAC)
/// - GL_UNSIGNED_INT(0x1405) 配 GL_DEPTH_COMPONENT24(0x81A6)
/// - GL_UNSIGNED_SHORT(0x1403) 配 GL_DEPTH_COMPONENT16(0x81A5)
///
/// 之前 normalize_internal_format 把 GL_DEPTH_COMPONENT 一律映射到 DEPTH_COMPONENT24，
/// 当 MC 传 type=GL_FLOAT 时（depth renderbuffer 纹理），GLES 报
/// "pixel buffer format is not compatible with level format"。
/// 此函数仅在 internalformat 为深度类格式时根据 type 精确选择，其余情况回退到
/// normalize_internal_format。
fn normalize_depth_internal_format(internalformat: u32, type_: u32) -> u32 {
    match internalformat {
        GL_DEPTH_COMPONENT => match type_ {
            GL_FLOAT => GL_DEPTH_COMPONENT32F,
            GL_UNSIGNED_INT => GL_DEPTH_COMPONENT24,
            GL_UNSIGNED_SHORT => GL_DEPTH_COMPONENT16,
            _ => GL_DEPTH_COMPONENT24,
        },
        // 已是 sized 深度格式或非深度格式：交给通用归一化
        _ => normalize_internal_format(internalformat),
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
    log::debug!("[FluorateGL] glGenTextures(n={})", n);
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
    log::debug!(
        "[FluorateGL] glBindTexture(target=0x{:04X}, texture={})",
        target,
        texture
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if texture == 0 {
            0
        } else {
            state::with_state(|s| {
                s.textures.get_gles(texture).unwrap_or_else(|| {
                log::warn!(
                    "[FluorateGL] glBindTexture(0x{:04X}, {}): desktop ID not found in IdMap, unbinding",
                    target, texture
                );
                0
            })
            })
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
    let normalized = normalize_depth_internal_format(internalformat as u32, type_) as i32;
    log::debug!(
        "[FluorateGL] glTexImage2D(target=0x{:04X}, level={}, internalformat=0x{:04X}, {}x{}, format=0x{:04X}, type=0x{:04X}, pixels={:?})",
        target,
        level,
        internalformat,
        width,
        height,
        format,
        type_,
        pixels
    );
    if normalized != internalformat {
        log::debug!(
            "[FluorateGL] glTexImage2D: normalized internalformat 0x{:04X} -> 0x{:04X}",
            internalformat,
            normalized
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
    log::debug!(
        "[FluorateGL] glTexParameteri(target=0x{:04X}, pname=0x{:04X}, param={})",
        target,
        pname,
        param
    );
    if is_unsupported_tex_parameter(pname) {
        log::debug!(
            "[FluorateGL] glTexParameteri pname 0x{:04X} ignored (unsupported in GLES)",
            pname
        );
        return;
    }
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
    let normalized = normalize_depth_internal_format(internalformat as u32, type_) as i32;
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
    log::debug!(
        "[FluorateGL] glTexStorage2D(target=0x{:04X}, levels={}, internalformat=0x{:04X}, {}x{})",
        target,
        levels,
        internalformat,
        width,
        height
    );
    if normalized != internalformat {
        log::debug!(
            "[FluorateGL] glTexStorage2D: normalized internalformat 0x{:04X} -> 0x{:04X}",
            internalformat,
            normalized
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
    log::debug!(
        "[FluorateGL] glTexParameterf(target=0x{:04X}, pname=0x{:04X}, param={})",
        target,
        pname,
        param
    );
    if is_unsupported_tex_parameter(pname) {
        log::debug!(
            "[FluorateGL] glTexParameterf pname 0x{:04X} ignored (unsupported in GLES)",
            pname
        );
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_parameter_f)(target, pname, param);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameterfv(target: u32, pname: u32, params: *const f32) {
    if is_unsupported_tex_parameter(pname) {
        log::debug!(
            "[FluorateGL] glTexParameterfv pname 0x{:04X} ignored (unsupported in GLES)",
            pname
        );
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_parameter_fv)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameteriv(target: u32, pname: u32, params: *const i32) {
    if is_unsupported_tex_parameter(pname) {
        log::debug!(
            "[FluorateGL] glTexParameteriv pname 0x{:04X} ignored (unsupported in GLES)",
            pname
        );
        return;
    }
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
    log::debug!(
        "[FluorateGL] glCompressedTexImage2D(target=0x{:04X}, level={}, internalformat=0x{:04X}, {}x{}, imageSize={}, data={:?})",
        target,
        level,
        internalformat,
        width,
        height,
        imageSize,
        data
    );

    // 防止将非压缩格式透传给 GLES 导致 GL_INVALID_ENUM 崩溃
    if !is_compressed_format(internalformat) {
        let normalized = normalize_internal_format(internalformat);
        log::warn!(
            "[FluorateGL] glCompressedTexImage2D: internalformat 0x{:04X} is not a compressed format, normalizing to 0x{:04X} and using glTexImage2D instead",
            internalformat,
            normalized
        );
        // 对非压缩格式降级为 glTexImage2D（data 指针直接复用，格式兼容）
        backend::with_gles_dispatch(|dispatch| unsafe {
            (dispatch.tex_image_2d)(
                target,
                level,
                normalized as i32,
                width,
                height,
                border,
                GL_RGBA,
                0x1401, // GL_UNSIGNED_BYTE
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
    log::debug!(
        "[FluorateGL] glCompressedTexImage3D(target=0x{:04X}, level={}, internalformat=0x{:04X}, {}x{}x{}, imageSize={}, data={:?})",
        target,
        level,
        internalformat,
        width,
        height,
        depth,
        imageSize,
        data
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
    if pixels.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.get_tex_image as *const ()) {
            // GLES 没有 glGetTexImage，用 FBO + glReadPixels 模拟
            emulate_get_tex_image(dispatch, target, level, format, type_, pixels);
            return;
        }
        (dispatch.get_tex_image)(target, level, format, type_, pixels);
    });
}

/// 用 FBO + glReadPixels 模拟 glGetTexImage。
///
/// 流程：
/// 1. 查询当前绑定到 target 的 GLES 纹理 ID 及该 level 宽高；
/// 2. 保存当前 FBO / 读缓冲绑定；
/// 3. 创建临时 FBO，把纹理该 level 挂到 COLOR_ATTACHMENT0；
/// 4. glReadPixels 读回像素；
/// 5. 删除临时 FBO，恢复 FBO / 读缓冲绑定。
///
/// 仅支持 2D 纹理与立方体贴理各面（MC 实际场景）。3D/2D_ARRAY 需按 layer 循环，
/// 而 glGetTexImage 不带 layer 参数，无法精确模拟，告警并跳过。
///
/// 局限：GLES glReadPixels 的 format/type 组合受限（如 GL_RGB 非必备读回格式），
/// 驱动不支持时像素数据未定义——这是模拟的固有约束，完整 CPU 格式转换超出范围。
unsafe fn emulate_get_tex_image(
    dispatch: &backend::dispatch::GlesDispatch,
    target: u32,
    level: i32,
    format: u32,
    type_: u32,
    pixels: *mut std::ffi::c_void,
) {
    // Rust 2024 edition：unsafe fn 内调用 unsafe 操作需显式 unsafe 块。
    unsafe {
        const GL_FRAMEBUFFER: u32 = 0x8D40;
        const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
        const GL_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
        const GL_READ_BUFFER: u32 = 0x0C02;
        const GL_TEXTURE_BINDING_2D: u32 = 0x8069;
        const GL_TEXTURE_BINDING_CUBE_MAP: u32 = 0x8514;
        const GL_TEXTURE_BINDING_3D: u32 = 0x806A;
        const GL_TEXTURE_BINDING_2D_ARRAY: u32 = 0x9108;
        const GL_TEXTURE_WIDTH: u32 = 0x1000;
        const GL_TEXTURE_HEIGHT: u32 = 0x1001;
        const GL_TEXTURE_2D: u32 = 0x0DE1;
        const GL_TEXTURE_CUBE_MAP_POSITIVE_X: u32 = 0x8515;
        const GL_TEXTURE_CUBE_MAP_NEGATIVE_Z: u32 = 0x851A;
        const GL_TEXTURE_3D: u32 = 0x806F;
        const GL_TEXTURE_2D_ARRAY: u32 = 0x8C03;
        const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;

        // 1. 解析 target → 绑定查询 pname + 归类
        let (binding_pname, is_3d_like) = match target {
            GL_TEXTURE_2D => (GL_TEXTURE_BINDING_2D, false),
            t if t >= GL_TEXTURE_CUBE_MAP_POSITIVE_X && t <= GL_TEXTURE_CUBE_MAP_NEGATIVE_Z => {
                (GL_TEXTURE_BINDING_CUBE_MAP, false)
            }
            GL_TEXTURE_3D => (GL_TEXTURE_BINDING_3D, true),
            GL_TEXTURE_2D_ARRAY => (GL_TEXTURE_BINDING_2D_ARRAY, true),
            _ => {
                log::warn!(
                    "[FluorateGL] glGetTexImage: 不支持的 target 0x{:04X}，已跳过",
                    target
                );
                return;
            }
        };

        if is_3d_like {
            log::warn!(
                "[FluorateGL] glGetTexImage: 3D/2D_ARRAY (target 0x{:04X}) 无 layer 参数无法精确模拟，已跳过",
                target
            );
            return;
        }

        // 深度格式纹理走 COLOR 附件不正确，告警跳过
        if format == GL_DEPTH_COMPONENT {
            log::warn!(
                "[FluorateGL] glGetTexImage: 深度格式读取未模拟，已跳过 (target 0x{:04X})",
                target
            );
            return;
        }

        let mut tex = 0i32;
        (dispatch.get_integerv)(binding_pname, &mut tex);
        if tex <= 0 {
            log::warn!(
                "[FluorateGL] glGetTexImage: target 0x{:04X} 当前无绑定纹理，已跳过",
                target
            );
            return;
        }

        // 2. 查询该 level 宽高
        let mut width = 0i32;
        let mut height = 0i32;
        (dispatch.get_tex_level_parameter_iv)(target, level, GL_TEXTURE_WIDTH, &mut width);
        (dispatch.get_tex_level_parameter_iv)(target, level, GL_TEXTURE_HEIGHT, &mut height);
        if width <= 0 || height <= 0 {
            log::debug!(
                "[FluorateGL] glGetTexImage: level {} 纹理尺寸 {}x{} 无效，已跳过",
                level,
                width,
                height
            );
            return;
        }

        // 3. 保存当前 FBO / 读缓冲
        let mut prev_fbo = 0i32;
        let mut prev_read_buffer = 0i32;
        (dispatch.get_integerv)(GL_FRAMEBUFFER_BINDING, &mut prev_fbo);
        (dispatch.get_integerv)(GL_READ_BUFFER, &mut prev_read_buffer);

        // 4. 创建临时 FBO 并挂载纹理该 level
        let mut fbo = 0u32;
        (dispatch.gen_framebuffers)(1, &mut fbo);
        (dispatch.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
        (dispatch.framebuffer_texture_2d)(
            GL_FRAMEBUFFER,
            GL_COLOR_ATTACHMENT0,
            target,
            tex as u32,
            level,
        );

        let status = (dispatch.check_framebuffer_status)(GL_FRAMEBUFFER);
        if status != GL_FRAMEBUFFER_COMPLETE {
            log::warn!(
                "[FluorateGL] glGetTexImage: FBO 不完整 (status=0x{:04X})，无法读回纹理，已跳过",
                status
            );
            (dispatch.delete_framebuffers)(1, &fbo);
            (dispatch.bind_framebuffer)(GL_FRAMEBUFFER, prev_fbo as u32);
            return;
        }

        // 5. 读回像素
        (dispatch.read_buffer)(GL_COLOR_ATTACHMENT0);
        (dispatch.read_pixels)(0, 0, width, height, format, type_, pixels);

        // 6. 清理并恢复
        (dispatch.delete_framebuffers)(1, &fbo);
        (dispatch.bind_framebuffer)(GL_FRAMEBUFFER, prev_fbo as u32);
        (dispatch.read_buffer)(prev_read_buffer as u32);

        log::debug!(
            "[FluorateGL] glGetTexImage 模拟完成: target=0x{:04X} level={} {}x{} format=0x{:04X} type=0x{:04X}",
            target,
            level,
            width,
            height,
            format,
            type_
        );
    } // unsafe
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
        let gles_id = state::with_state(|s| s.textures.get_gles(texture).unwrap_or(0));
        if gles_id == 0 {
            return 0;
        }
        (dispatch.is_texture)(gles_id)
    })
}
