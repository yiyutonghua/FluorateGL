use crate::backend;

/// glClampColor stub — GL 3.2 固定功能颜色 clamp 控制。
///
/// GLES 总是 clamp 颜色输出（ framebuffer 写入前 clamp 到 [0,1]），
/// 行为与桌面 GL 的 GL_CLAMP_READ_COLOR / GL_FIXED_ONLY 语义一致，
/// 故无需转发，直接 no-op。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClampColor(_target: u32, _clamp: u32) {
    log::debug!("[FluorateGL] glClampColor swallowed (GLES always clamps color output)");
}

/// glPointParameteri — GL 1.4 点光栅化参数（int 版本）。
///
/// 转发到 GLES 的 glPointParameterf（GLES 仅提供 float 版本）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPointParameteri(pname: u32, param: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.point_parameter_f)(pname, param as f32);
    });
}

/// glPointParameteriv — GL 1.4 点光栅化参数（int 数组版本）。
///
/// GLES 仅提供 glPointParameterf，故取数组首元素转为 float 转发。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPointParameteriv(pname: u32, params: *const i32) {
    if params.is_null() {
        return;
    }
    let param = unsafe { *params };
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.point_parameter_f)(pname, param as f32);
    });
}

// ==== pixel-store 影子状态（移植自 MobileGlues gl/pixel.cpp）====
//
// 桌面 GL 有 6 个 GLES 不存在的 pixel-store 参数：
//   GL_UNPACK_SWAP_BYTES / GL_UNPACK_LSB_FIRST / GL_PACK_SWAP_BYTES /
//   GL_PACK_LSB_FIRST / GL_PACK_IMAGE_HEIGHT / GL_PACK_SKIP_IMAGES
// 直通驱动会 INVALID_ENUM 且无法读回（glGetIntegerv 同样失败不写 data）。
// 这 6 个参数存影子表（set 存储 / query_int 读回）；其余 GLES 认识的参数
// 仍由驱动持有，本层仅透传（对齐 MG pixel.cpp 注释：6 个是"存储"，
// GLES 有的参数是"镜像"，我们无 unpack 镜像消费者，texture 上传在域 2）。
//
// 线程归属：GL pixel-store 状态属于当前上下文，与 state::State 一致
// 用 thread_local（MG 用 per-context gl_state + thread_local 镜像）。
pub(crate) mod pixel_store {
    use std::cell::RefCell;

    const GL_UNPACK_SWAP_BYTES: u32 = 0x0CF0;
    const GL_UNPACK_LSB_FIRST: u32 = 0x0CF1;
    const GL_PACK_SWAP_BYTES: u32 = 0x0D00;
    const GL_PACK_LSB_FIRST: u32 = 0x0D01;
    const GL_PACK_IMAGE_HEIGHT: u32 = 0x806C;
    const GL_PACK_SKIP_IMAGES: u32 = 0x806B;
    const GL_INVALID_VALUE: u32 = 0x0501;

    #[derive(Clone, Copy, Default)]
    struct PixelStoreShadow {
        unpack_swap_bytes: i32,
        unpack_lsb_first: i32,
        pack_swap_bytes: i32,
        pack_lsb_first: i32,
        pack_image_height: i32,
        pack_skip_images: i32,
    }

    thread_local! {
        static SHADOW: RefCell<PixelStoreShadow> = RefCell::new(PixelStoreShadow::default());
    }

    /// 桌面 6 参数存储；返回 true 表示已由影子表处理（调用方不再转发驱动）。
    ///
    /// 对齐 MG mg_pixel_store_set：布尔参数存 0/1；PACK_IMAGE_HEIGHT /
    /// PACK_SKIP_IMAGES 为计数，负值报 GL_INVALID_VALUE 且不存储
    /// （GL 规范拒绝负计数，驱动本会说——但影子表吞掉了转发，
    /// 故由前端错误槽补报；对齐 MG：错误值不进影子）。
    pub(crate) fn set(pname: u32, param: i32) -> bool {
        match pname {
            GL_PACK_IMAGE_HEIGHT | GL_PACK_SKIP_IMAGES if param < 0 => {
                crate::gl::exports::set_gl_error(GL_INVALID_VALUE);
                true
            }
            GL_UNPACK_SWAP_BYTES => {
                SHADOW.with(|cell| {
                    cell.borrow_mut().unpack_swap_bytes = if param != 0 { 1 } else { 0 }
                });
                true
            }
            GL_UNPACK_LSB_FIRST => {
                SHADOW.with(|cell| {
                    cell.borrow_mut().unpack_lsb_first = if param != 0 { 1 } else { 0 }
                });
                true
            }
            GL_PACK_SWAP_BYTES => {
                SHADOW.with(|cell| {
                    cell.borrow_mut().pack_swap_bytes = if param != 0 { 1 } else { 0 }
                });
                true
            }
            GL_PACK_LSB_FIRST => {
                SHADOW
                    .with(|cell| cell.borrow_mut().pack_lsb_first = if param != 0 { 1 } else { 0 });
                true
            }
            GL_PACK_IMAGE_HEIGHT => {
                SHADOW.with(|cell| cell.borrow_mut().pack_image_height = param);
                true
            }
            GL_PACK_SKIP_IMAGES => {
                SHADOW.with(|cell| cell.borrow_mut().pack_skip_images = param);
                true
            }
            _ => false,
        }
    }

    /// 读回桌面 6 参数；返回 true 表示是影子表参数（*out 已写入）。
    /// 供 glGetIntegerv / glGetBooleanv / glGetFloatv / glGetDoublev /
    /// glGetInteger64v 统一回答（对齐 MG mg_pixel_store_query_int）。
    pub(crate) fn query_int(pname: u32, out: &mut i32) -> bool {
        let slot = SHADOW.with(|cell| match pname {
            GL_UNPACK_SWAP_BYTES => Some(cell.borrow().unpack_swap_bytes),
            GL_UNPACK_LSB_FIRST => Some(cell.borrow().unpack_lsb_first),
            GL_PACK_SWAP_BYTES => Some(cell.borrow().pack_swap_bytes),
            GL_PACK_LSB_FIRST => Some(cell.borrow().pack_lsb_first),
            GL_PACK_IMAGE_HEIGHT => Some(cell.borrow().pack_image_height),
            GL_PACK_SKIP_IMAGES => Some(cell.borrow().pack_skip_images),
            _ => None,
        });
        if let Some(v) = slot {
            *out = v;
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    /// GL_UNPACK_SWAP_BYTES 是否对给定 type 生效（字节数 > 1 时）。
    ///
    /// 移植 MG mg_unpack_swaps_bytes：供纹理上传（域 2 texture.rs）在需要
    /// repack 时判定；本层不消费。注意：SWAP_BYTES 只存储不实现——
    /// 影子表如实记录参数，但本层不做字节交换（对齐 MG 注释：层不
    /// byte-swap，宿主喂 big-endian 数据仍原样上传，首次告警）。
    pub(crate) fn unpack_swaps_bytes(type_: u32) -> bool {
        let on = SHADOW.with(|cell| cell.borrow().unpack_swap_bytes != 0);
        on && super::gl_sizeof(type_) > 1
    }

    #[allow(dead_code)]
    /// GL_PACK_SWAP_BYTES 判定（同上，读回路径用）。
    pub(crate) fn pack_swaps_bytes(type_: u32) -> bool {
        let on = SHADOW.with(|cell| cell.borrow().pack_swap_bytes != 0);
        on && super::gl_sizeof(type_) > 1
    }
}

// ==== pixel-transfer 尺寸表（移植自 MobileGlues gl/pixel.cpp）====
//
// 三张必须一致的表（MG 注释）：
// - gl_sizeof：按 transfer TYPE 回答字节数（非 packed 类型按分量、
//   packed 类型按整像素）
// - is_type_packed：TYPE 是否为 packed（整像素装一个单元）
// - pixel_sizeof：FORMAT + TYPE → 每像素字节数
// 本域无上传路径消费者（texture.rs 域 2 可能使用）；SWAP_BYTES 判定
// （unpack_swaps_bytes / pack_swaps_bytes）依赖 gl_sizeof。

#[allow(dead_code)]
/// 每个 TYPE 单元的字节数（对照 MG gl_sizeof；不认识的 type 返回 0）。
pub(crate) fn gl_sizeof(type_: u32) -> i32 {
    match type_ {
        0x140A /* GL_DOUBLE */ | 0x8DAD /* GL_FLOAT_32_UNSIGNED_INT_24_8_REV */ => 8,
        0x1406 /* GL_FLOAT */ | 0x1404 /* GL_INT */ | 0x1405 /* GL_UNSIGNED_INT */
        | 0x8037 /* GL_UNSIGNED_INT_10_10_10_2 */ | 0x8368 /* GL_UNSIGNED_INT_2_10_10_10_REV */
        | 0x8035 /* GL_UNSIGNED_INT_8_8_8_8 */ | 0x8036 /* GL_UNSIGNED_INT_8_8_8_8_REV */
        | 0x84FA /* GL_UNSIGNED_INT_24_8 */
        | 0x8C3B /* GL_UNSIGNED_INT_10F_11F_11F_REV */ | 0x8C3E /* GL_UNSIGNED_INT_5_9_9_9_REV */
        | 0x1407 /* GL_4_BYTES */ => 4,
        0x1408 /* GL_3_BYTES */ => 3,
        0x1402 /* GL_SHORT */ | 0x140B /* GL_HALF_FLOAT */ | 0x1403 /* GL_UNSIGNED_SHORT */
        | 0x8366 /* GL_UNSIGNED_SHORT_1_5_5_5_REV */ | 0x8033 /* GL_UNSIGNED_SHORT_4_4_4_4 */
        | 0x8365 /* GL_UNSIGNED_SHORT_4_4_4_4_REV */ | 0x8034 /* GL_UNSIGNED_SHORT_5_5_5_1 */
        | 0x8363 /* GL_UNSIGNED_SHORT_5_6_5 */ | 0x8364 /* GL_UNSIGNED_SHORT_5_6_5_REV */
        | 0x1409 /* GL_2_BYTES */ => 2,
        0x1400 /* GL_BYTE */ | 0x1401 /* GL_UNSIGNED_BYTE */
        | 0x8362 /* GL_UNSIGNED_BYTE_2_3_3_REV */ | 0x8032 /* GL_UNSIGNED_BYTE_3_3_2 */ => 1,
        _ => 0,
    }
}

#[allow(dead_code)]
/// TYPE 是否为 packed（整像素装在一个单元里；对照 MG is_type_packed）。
pub(crate) fn is_type_packed(type_: u32) -> bool {
    matches!(
        type_,
        0x1407 | // GL_4_BYTES
        0x1408 | // GL_3_BYTES
        0x1409 | // GL_2_BYTES
        0x8362 | // GL_UNSIGNED_BYTE_2_3_3_REV
        0x8032 | // GL_UNSIGNED_BYTE_3_3_2
        0x8037 | // GL_UNSIGNED_INT_10_10_10_2
        0x8368 | // GL_UNSIGNED_INT_2_10_10_10_REV
        0x8035 | // GL_UNSIGNED_INT_8_8_8_8
        0x8036 | // GL_UNSIGNED_INT_8_8_8_8_REV
        0x84FA | // GL_UNSIGNED_INT_24_8
        0x8DAD | // GL_FLOAT_32_UNSIGNED_INT_24_8_REV
        0x8C3B | // GL_UNSIGNED_INT_10F_11F_11F_REV
        0x8C3E | // GL_UNSIGNED_INT_5_9_9_9_REV
        0x8366 | // GL_UNSIGNED_SHORT_1_5_5_5_REV
        0x8033 | // GL_UNSIGNED_SHORT_4_4_4_4
        0x8365 | // GL_UNSIGNED_SHORT_4_4_4_4_REV
        0x8034 | // GL_UNSIGNED_SHORT_5_5_5_1
        0x8363 | // GL_UNSIGNED_SHORT_5_6_5
        0x8364 // GL_UNSIGNED_SHORT_5_6_5_REV
    )
}

#[allow(dead_code)] // 跨域接口预留（pixel store 影子实现，当前无调用方）
/// FORMAT + TYPE → 每像素字节数（对照 MG pixel_sizeof；
/// packed type 时分量数折叠为 1；不认识的组合返回 0）。
pub(crate) fn pixel_sizeof(format: u32, type_: u32) -> i32 {
    let width: i32 = match format {
        0x1903 /* GL_RED */ | 0x1904 /* GL_GREEN */ | 0x1905 /* GL_BLUE */
        | 0x1906 /* GL_ALPHA */ | 0x1909 /* GL_LUMINANCE */ | 0x1902 /* GL_DEPTH_COMPONENT */
        | 0x84F9 /* GL_DEPTH_STENCIL */ | 0x1901 /* GL_STENCIL_INDEX */ | 0x1400 /* GL_COLOR_INDEX */
        | 0x8D94 /* GL_RED_INTEGER */ | 0x8D95 /* GL_GREEN_INTEGER */
        | 0x8D96 /* GL_BLUE_INTEGER */ | 0x8D97 /* GL_ALPHA_INTEGER */ => 1,
        0x8227 /* GL_RG */ | 0x190A /* GL_LUMINANCE_ALPHA */ | 0x8228 /* GL_RG_INTEGER */ => 2,
        0x1907 /* GL_RGB */ | 0x80E0 /* GL_BGR */ | 0x8D98 /* GL_RGB_INTEGER */
        | 0x8D9A /* GL_BGR_INTEGER */ => 3,
        0x1908 /* GL_RGBA */ | 0x80E1 /* GL_BGRA */ | 0x8D99 /* GL_RGBA_INTEGER */
        | 0x8D9B /* GL_BGRA_INTEGER */ => 4,
        _ => return 0,
    };
    // packed type 已含整像素，分量数折叠
    let w = if is_type_packed(type_) { 1 } else { width };
    w * gl_sizeof(type_)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gl_sizeof_known_types() {
        assert_eq!(gl_sizeof(0x1401), 1); // GL_UNSIGNED_BYTE
        assert_eq!(gl_sizeof(0x1406), 4); // GL_FLOAT
        assert_eq!(gl_sizeof(0x140A), 8); // GL_DOUBLE
        assert_eq!(gl_sizeof(0xDEAD), 0); // 未知
    }

    #[test]
    fn pixel_sizeof_rgba_u8() {
        assert_eq!(pixel_sizeof(0x1908, 0x1401), 4); // RGBA + UNSIGNED_BYTE
        assert_eq!(pixel_sizeof(0x1907, 0x1401), 3); // RGB + UNSIGNED_BYTE
    }

    #[test]
    fn packed_type_collapses_components() {
        // BGRA + UNSIGNED_INT_8_8_8_8_REV（packed）：整像素 4 字节
        assert_eq!(pixel_sizeof(0x80E1, 0x8036), 4);
        assert!(is_type_packed(0x8036));
        assert!(!is_type_packed(0x1401));
    }

    #[test]
    fn pixel_store_shadow_roundtrip() {
        use pixel_store::{query_int, set};
        assert!(set(0x0CF0, 1)); // GL_UNPACK_SWAP_BYTES → 影子
        assert!(!set(0x0D05, 4)); // GL_PACK_ALIGNMENT → 非影子（转发驱动）
        let mut out = 0i32;
        assert!(query_int(0x0CF0, &mut out));
        assert_eq!(out, 1);
        assert!(!query_int(0x0D05, &mut out));
        // 负计数 → 前端错误槽 + 不存储（对齐 MG：错误值不进影子）
        assert!(set(0x806C, -1)); // GL_PACK_IMAGE_HEIGHT 负值被拒
        let mut neg = 1i32;
        assert!(query_int(0x806C, &mut neg));
        assert_eq!(neg, 0, "负计数不应写入影子");
        // 恢复默认（thread_local 每测试线程独立，但防御性复位）
        let _ = set(0x0CF0, 0);
    }
}
