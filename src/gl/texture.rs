//! 桌面 OpenGL 纹理对象拦截层（MobileGlues 语义移植版）
//!
//! 本文件是 MobileGlues（MG）`gl/texture.cpp` 的 Rust 移植，替换原实现：
//!
//! - **internal_convert**（MG texture.cpp:494-815）：内部格式/类型/格式三元组转换，
//!   深度格式按像素 type 选择 sized 格式、16 位格式按 GL_EXT_texture_norm16
//!   能力降级、unsized GL_RED 按 type 推导、GL_BGRA 交由 CPU 转换等。
//! - **CPU 上传转换**（MG gl/transfer.cpp `mg_upload_fix_t`）：BGRA/BGR/8888 packed
//!   等 GLES 没有的客户端格式在传输边界做 CPU 重排（含 unpack PBO 读取、unpack
//!   状态保存/恢复），GLES 只看到合法格式。
//! - **GetTexImage 读回**（MG texture.cpp:1619）：临时 FBO + glReadPixels 模拟，
//!   含读回 pair 支持检查与 RGBA→目标格式 CPU 编码（MG gl/transfer.cpp 读回侧）。
//! - **swizzle 跟踪**（MG texture.cpp:1430）：GL_TEXTURE_SWIZZLE_RGBA 展开为
//!   R/G/B/A 四次调用并记录影子状态。
//! - **深度纹理拷贝**（MG texture.cpp:1161/1250）：glCopyTexImage2D/SubImage2D
//!   的深度路径用临时 DRAW FBO + glBlitFramebuffer 模拟。
//! - **glClearTexImage**（MG texture.cpp:1881）：临时 FBO + glClear 真实实现。
//! - **对象表**：MG 的 TextureObject 影子记录（target/internal_format/尺寸/swizzle），
//!   以 desktop ID 为 key（我们的 IdMap 负责 ID 翻译，接口保持不变；
//!   framebuffer 附件仍引用 desktop 纹理 ID）。
//!
//! 保留的原实现体系（不属替换范围）：
//! - 独有告警体系（首次告警标志 + 静默降级日志）
//! - S3TC 驱动能力检查与忽略、非压缩格式降级 glTexImage2D
//! - IdMap（desktop ↔ GLES ID 翻译；MG 的 fake==real ID 方案与 framebuffer 域
//!   的附件引用冲突，接口冻结要求下保留）
//! - 1D/multisample 系列 no-op stub 语义（MG 同样为 stub）

use crate::backend;
use crate::state;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// texture desktop ID 查找失败首次告警标志
static TEXTURE_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);
/// glCompressedTexImage 收到非压缩格式首次告警标志
static COMPRESSED_FORMAT_MISMATCH_WARNED: AtomicBool = AtomicBool::new(false);
/// S3TC 上传因驱动不支持被忽略的首次告警标志
static S3TC_UNSUPPORTED_WARNED: AtomicBool = AtomicBool::new(false);
/// 上传 CPU 转换因 unpack PBO 不可读被丢弃的首次告警标志
static UPLOAD_PBO_UNREADABLE_WARNED: AtomicBool = AtomicBool::new(false);
/// 上传 CPU 转换尺寸溢出被丢弃的首次告警标志
static UPLOAD_SIZE_OVERFLOW_WARNED: AtomicBool = AtomicBool::new(false);
/// GL_UNPACK_IMAGE_HEIGHT/SKIP_IMAGES 未跟踪的首次告警标志
static UNPACK_IMAGE_HEIGHT_UNTRACKED_WARNED: AtomicBool = AtomicBool::new(false);
/// glGetTexImage 读回 pair 不支持的首次告警标志
static READBACK_PAIR_UNSUPPORTED_WARNED: AtomicBool = AtomicBool::new(false);
/// glGetTexImage RGBA 读回失败的首次告警标志
static READBACK_RGBA_FAILED_WARNED: AtomicBool = AtomicBool::new(false);
/// glGetTexImage pack PBO 不可写且不可 subdata 的首次告警标志
static READBACK_PBO_UNWRITABLE_WARNED: AtomicBool = AtomicBool::new(false);
/// glClearTexImage 无法解析清除值的首次告警标志
static CLEAR_VALUE_UNDECODABLE_WARNED: AtomicBool = AtomicBool::new(false);
/// glClearTexImage 无法挂载纹理的首次告警标志
static CLEAR_ATTACH_FAILED_WARNED: AtomicBool = AtomicBool::new(false);
/// glCopyTexSubImage2D 深度目标无法构成完整 FBO 的首次告警标志
static COPY_DEPTH_FBO_INCOMPLETE_WARNED: AtomicBool = AtomicBool::new(false);
/// glCopyTexSubImage2D 深度 blit 被驱动拒绝的首次告警标志
static COPY_DEPTH_BLIT_REFUSED_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：texture desktop ID 未在 IdMap 中找到。
fn warn_texture_id_miss(fname: &str, target: u32, desktop_id: u32) {
    if !TEXTURE_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: target 0x{:04X} desktop ID {} not found in IdMap, unbinding (跨线程或资源已释放，后续将静默降级)",
            fname,
            target,
            desktop_id
        );
    }
}

/// 首次告警：glCompressedTexImage 收到非压缩格式，已降级为 glTexImage2D。
fn warn_compressed_format_mismatch(fname: &str, internalformat: u32, normalized: u32) {
    if !COMPRESSED_FORMAT_MISMATCH_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: internalformat 0x{:04X} is not a compressed format, normalizing to 0x{:04X} and using glTexImage2D instead (后续调用将静默降级)",
            fname,
            internalformat,
            normalized
        );
    }
}

/// 判断 dispatch 函数指针是否为共享的未实现 stub。
///
/// `load_opt!` 把缺失的可选函数替换为同一个 stub 函数，故 GlesDispatch 中所有 stub
/// 字段地址相同。与 `dispatch.stub` 比较即可判定该 GLES 函数是否被驱动支持。
fn is_stub(dispatch: &backend::dispatch::GlesDispatch, ptr: *const ()) -> bool {
    ptr == dispatch.stub as *const ()
}

// ===========================================================================
// GL 常量（含桌面 GL 独有、GLES 无对应枚举的，用于 CPU 模拟）
// ===========================================================================

const GL_RED: u32 = 0x1903;
const GL_GREEN: u32 = 0x1904;
const GL_BLUE: u32 = 0x1905;
const GL_ALPHA: u32 = 0x1906;
const GL_RGB: u32 = 0x1907;
const GL_RGBA: u32 = 0x1908;
const GL_LUMINANCE: u32 = 0x1909;
const GL_LUMINANCE_ALPHA: u32 = 0x190A;
const GL_RG: u32 = 0x8227;
const GL_DEPTH_COMPONENT: u32 = 0x1902;
const GL_DEPTH_STENCIL: u32 = 0x84F9;
const GL_STENCIL_INDEX: u32 = 0x1901;

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
const GL_RGB10_A2: u32 = 0x8059;
const GL_RGB12: u32 = 0x8053;
const GL_RGB16: u32 = 0x8054;
const GL_RGBA2: u32 = 0x8055;
const GL_RGBA4: u32 = 0x8056;
const GL_RGBA12: u32 = 0x805A;
const GL_RGBA16: u32 = 0x805B;
const GL_RGB16F: u32 = 0x881B;
const GL_RGBA16F: u32 = 0x881A;
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

// ---- MG internal_convert 相关常量 ----
const GL_BYTE: u32 = 0x1400;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_SHORT: u32 = 0x1402;
const GL_INT: u32 = 0x1404;
const GL_HALF_FLOAT: u32 = 0x140B;
const GL_UNSIGNED_SHORT_5_6_5: u32 = 0x8363;
const GL_UNSIGNED_SHORT_5_5_5_1: u32 = 0x8034;
const GL_UNSIGNED_INT_2_10_10_10_REV: u32 = 0x8368;
const GL_UNSIGNED_INT_24_8: u32 = 0x84FA;
const GL_FLOAT_32_UNSIGNED_INT_24_8_REV: u32 = 0x8DAD;
const GL_UNSIGNED_INT_5_9_9_9_REV: u32 = 0x8C3E;
const GL_UNSIGNED_INT_10F_11F_11F_REV: u32 = 0x8C3B;
const GL_UNSIGNED_INT_8_8_8_8: u32 = 0x8035;
const GL_UNSIGNED_INT_8_8_8_8_REV: u32 = 0x8367;
const GL_UNSIGNED_SHORT_1_5_5_5_REV: u32 = 0x8366;
const GL_UNSIGNED_SHORT_4_4_4_4_REV: u32 = 0x8365;
const GL_DEPTH32F_STENCIL8: u32 = 0x8CAD;
const GL_RGB5_A1: u32 = 0x8057;
const GL_SRGB8: u32 = 0x8C41;
const GL_RGB9_E5: u32 = 0x8C3D;
const GL_R11F_G11F_B10F: u32 = 0x8C3A;
const GL_RGBA32F: u32 = 0x8814;
const GL_RGB32F: u32 = 0x8815;
const GL_RGBA32UI: u32 = 0x8D70;
const GL_RGB32UI: u32 = 0x8D71;
const GL_RGBA32I: u32 = 0x8D82;
const GL_RGB32I: u32 = 0x8D83;
const GL_R8I: u32 = 0x8231;
const GL_R8UI: u32 = 0x8232;
const GL_R16I: u32 = 0x8233;
const GL_R16UI: u32 = 0x8234;
const GL_R32I: u32 = 0x8235;
const GL_R32UI: u32 = 0x8236;
const GL_RG8I: u32 = 0x8237;
const GL_RG8UI: u32 = 0x8238;
const GL_RG16I: u32 = 0x8239;
const GL_RG16UI: u32 = 0x823A;
const GL_RG32I: u32 = 0x823B;
const GL_RG32UI: u32 = 0x823C;
const GL_R8_SNORM: u32 = 0x8F94;
const GL_RG8_SNORM: u32 = 0x8F95;
const GL_RGBA8_SNORM: u32 = 0x8F97;
const GL_RGBA16_SNORM: u32 = 0x8F9B;
const GL_R16: u32 = 0x822A;
const GL_R16F: u32 = 0x822D;
const GL_R32F: u32 = 0x822E;
const GL_RG16: u32 = 0x822C;
const GL_RG16F: u32 = 0x822F;
const GL_RG32F: u32 = 0x8230;
const GL_RED_INTEGER: u32 = 0x8D94;
const GL_RG_INTEGER: u32 = 0x8228;
const GL_COMPRESSED_RED_RGTC1: u32 = 0x8DBB;
const GL_COMPRESSED_RG_RGTC2: u32 = 0x8DBD;

#[allow(dead_code)]
#[allow(dead_code)]
// ---- 纹理目标 / 绑定查询 ----
const GL_TEXTURE_1D: u32 = 0x0DE0;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_TEXTURE_3D: u32 = 0x806F;
const GL_TEXTURE_CUBE_MAP: u32 = 0x8513;
const GL_TEXTURE_CUBE_MAP_POSITIVE_X: u32 = 0x8515;
const GL_TEXTURE_CUBE_MAP_NEGATIVE_Z: u32 = 0x851A;
const GL_TEXTURE_2D_ARRAY: u32 = 0x8C03;
const GL_TEXTURE_BUFFER: u32 = 0x8C2A;
const GL_TEXTURE_RECTANGLE: u32 = 0x84F5;
const GL_PROXY_TEXTURE_1D: u32 = 0x8063;
const GL_PROXY_TEXTURE_2D: u32 = 0x8064;
const GL_PROXY_TEXTURE_3D: u32 = 0x8070;
const GL_PROXY_TEXTURE_RECTANGLE: u32 = 0x84F7;
const GL_TEXTURE_BINDING_2D: u32 = 0x8069;
const GL_TEXTURE_BINDING_CUBE_MAP: u32 = 0x8514;
#[allow(dead_code)] // 预留（texture buffer emulate 场景）
const GL_TEXTURE_BINDING_3D: u32 = 0x806A;
#[allow(dead_code)] // 预留（texture buffer emulate 场景）
const GL_TEXTURE_BINDING_2D_ARRAY: u32 = 0x9108;
const GL_TEXTURE_WIDTH: u32 = 0x1000;
const GL_TEXTURE_HEIGHT: u32 = 0x1001;
const GL_TEXTURE_DEPTH: u32 = 0x8071;
const GL_TEXTURE_INTERNAL_FORMAT: u32 = 0x1003;
const GL_MAX_TEXTURE_SIZE: u32 = 0x0D33;
const GL_ACTIVE_TEXTURE: u32 = 0x84E0;
const GL_TEXTURE0: u32 = 0x84C0;

// ---- 纹理参数 ----
const GL_TEXTURE_SWIZZLE_RGBA: u32 = 0x8C43;
const GL_TEXTURE_SWIZZLE_R: u32 = 0x8E42;
const GL_TEXTURE_SWIZZLE_G: u32 = 0x8E43;
const GL_TEXTURE_SWIZZLE_B: u32 = 0x8E44;
const GL_TEXTURE_SWIZZLE_A: u32 = 0x8E45;
const GL_TEXTURE_LOD_BIAS_QCOM: u32 = 0x8C29;

// ---- 帧缓冲（临时 FBO 模拟用）----
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_READ_FRAMEBUFFER: u32 = 0x8CA8;
const GL_DRAW_FRAMEBUFFER: u32 = 0x8CA9;
const GL_READ_FRAMEBUFFER_BINDING: u32 = 0x8CAA;
const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA9;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_DEPTH_ATTACHMENT: u32 = 0x8D00;
const GL_STENCIL_ATTACHMENT: u32 = 0x8D20;
const GL_DEPTH_STENCIL_ATTACHMENT: u32 = 0x821A;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_DEPTH_BUFFER_BIT: u32 = 0x100;
const GL_STENCIL_BUFFER_BIT: u32 = 0x400;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_NEAREST: u32 = 0x2600;

// ---- 像素传输（CPU 转换用）----
const GL_PIXEL_UNPACK_BUFFER: u32 = 0x88EC;
const GL_PIXEL_PACK_BUFFER: u32 = 0x88EB;
const GL_PIXEL_UNPACK_BUFFER_BINDING: u32 = 0x88EF;
const GL_PIXEL_PACK_BUFFER_BINDING: u32 = 0x88ED;
const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;
const GL_UNPACK_ROW_LENGTH: u32 = 0x0CF2;
const GL_UNPACK_SKIP_ROWS: u32 = 0x0CF3;
const GL_UNPACK_SKIP_PIXELS: u32 = 0x0CF4;
const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_PACK_ROW_LENGTH: u32 = 0x0D02;
const GL_PACK_SKIP_ROWS: u32 = 0x0D03;
const GL_PACK_SKIP_PIXELS: u32 = 0x0D04;
const GL_MAP_READ_BIT: u32 = 0x1;
const GL_MAP_WRITE_BIT: u32 = 0x2;
const GL_STREAM_READ: u32 = 0x88E0;
const GL_COPY_WRITE_BUFFER: u32 = 0x8F37;
const GL_COPY_WRITE_BUFFER_BINDING: u32 = 0x8F38;
const GL_IMPLEMENTATION_COLOR_READ_FORMAT: u32 = 0x8B9B;
const GL_IMPLEMENTATION_COLOR_READ_TYPE: u32 = 0x8B9A;
const GL_COLOR_CLEAR_VALUE: u32 = 0x0C22;
const GL_DEPTH_CLEAR_VALUE: u32 = 0x0B73;
const GL_STENCIL_CLEAR_VALUE: u32 = 0x0B91;
const GL_NUM_EXTENSIONS: u32 = 0x821D;
const GL_EXTENSIONS: u32 = 0x1F03;

// ===========================================================================
// 驱动能力查询（MG g_gles_caps 的按需等价物，扩展名列表首次查询后缓存）
// ===========================================================================

/// GLES 扩展名列表缓存（首次查询后恒定，与 COMPRESSED_FORMATS_SUPPORTED 同模式）。
static GLES_EXTENSIONS: OnceLock<Vec<String>> = OnceLock::new();

fn gles_extensions(dispatch: &backend::dispatch::GlesDispatch) -> &'static Vec<String> {
    GLES_EXTENSIONS.get_or_init(|| {
        let mut num = 0i32;
        unsafe { (dispatch.get_integerv)(GL_NUM_EXTENSIONS, &mut num) };
        let mut exts = Vec::new();
        if num > 0 {
            for i in 0..num as u32 {
                let ptr = unsafe { (dispatch.get_string_i)(GL_EXTENSIONS, i) };
                if !ptr.is_null() {
                    if let Ok(s) = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str() {
                        exts.push(s.to_string());
                    }
                }
            }
        }
        exts
    })
}

fn gles_has_extension(dispatch: &backend::dispatch::GlesDispatch, name: &str) -> bool {
    gles_extensions(dispatch).iter().any(|e| e == name)
}

/// GL_EXT_texture_norm16 能力（internal_convert 的 16 位格式降级判断）。
fn gles_norm16(dispatch: &backend::dispatch::GlesDispatch) -> bool {
    static NORM16: OnceLock<bool> = OnceLock::new();
    *NORM16.get_or_init(|| gles_has_extension(dispatch, "GL_EXT_texture_norm16"))
}

/// GL_QCOM_texture_lod_bias 能力（glTexParameterf/i 的 LOD_BIAS 转发判断）。
fn gles_lod_bias_qcom(dispatch: &backend::dispatch::GlesDispatch) -> bool {
    static LOD_BIAS_QCOM: OnceLock<bool> = OnceLock::new();
    *LOD_BIAS_QCOM.get_or_init(|| gles_has_extension(dispatch, "GL_QCOM_texture_lod_bias"))
}

// ===========================================================================
// 对象表：纹理影子元数据（MG TextureObject 等价物）
// ===========================================================================

/// 每个纹理对象的影子状态（MG TextureObject：target/internal_format/format/尺寸/swizzle）。
///
/// 以 desktop ID 为 key（我们的 ID 由 IdMap 翻译，ID 接口与 framebuffer 附件
/// 引用保持不变；MG 的 fake==real ID 方案不适用）。与 State 同为 thread_local，
/// 跨线程访问的降级行为与 IdMap 一致（找不到时按默认值处理）。
#[derive(Clone, Copy, Debug)]
struct TextureMeta {
    /// 最近绑定的目标（cube face 归一化为 GL_TEXTURE_CUBE_MAP）
    target: u32,
    /// 最近分配的 internalformat（转换后，MG 语义）
    internal_format: u32,
    /// 最近分配的客户端 format
    format: u32,
    width: i32,
    height: i32,
    depth: i32,
    /// GL_TEXTURE_SWIZZLE_R/G/B/A 影子
    swizzle: [i32; 4],
}

impl Default for TextureMeta {
    fn default() -> Self {
        Self {
            target: GL_TEXTURE_2D,
            internal_format: 0,
            format: 0,
            width: 0,
            height: 0,
            depth: 1,
            swizzle: [
                GL_RED as i32,
                GL_GREEN as i32,
                GL_BLUE as i32,
                GL_ALPHA as i32,
            ],
        }
    }
}

thread_local! {
    /// desktop ID → 纹理影子元数据
    static TEXTURE_META: RefCell<FxHashMap<u32, TextureMeta>> = RefCell::new(FxHashMap::default());
    /// 归一化 target → 当前绑定的 desktop 纹理 ID（glBindTexture 维护）
    static BOUND_TEXTURES: RefCell<FxHashMap<u32, u32>> = RefCell::new(FxHashMap::default());
}

fn meta_get(id: u32) -> TextureMeta {
    TEXTURE_META.with(|m| m.borrow().get(&id).copied().unwrap_or_default())
}

fn meta_get_mut<F, R>(id: u32, f: F) -> R
where
    F: FnOnce(&mut TextureMeta) -> R,
{
    TEXTURE_META.with(|m| {
        let mut map = m.borrow_mut();
        let entry = map.entry(id).or_default();
        f(entry)
    })
}

fn meta_remove(id: u32) {
    TEXTURE_META.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

/// 归一化纹理目标：cube 面统一为 GL_TEXTURE_CUBE_MAP（MG ConvertGLEnumToTextureTarget
/// 的核心折叠；其余目标原样返回）。
fn normalize_target(target: u32) -> u32 {
    if (GL_TEXTURE_CUBE_MAP_POSITIVE_X..=GL_TEXTURE_CUBE_MAP_NEGATIVE_Z).contains(&target) {
        GL_TEXTURE_CUBE_MAP
    } else {
        target
    }
}

fn bound_texture_for_target(target: u32) -> Option<u32> {
    BOUND_TEXTURES.with(|m| m.borrow().get(&normalize_target(target)).copied())
}

// ===========================================================================
// PROXY_TEXTURE_* 查询影子（MG gl_state proxy_width/height/intformat）
// ===========================================================================

/// PROXY 查询是纯 CPU 模拟（MG texture.cpp:924-932）：glTexImage* 收到
/// PROXY_TEXTURE_* target 时不发任何 GLES 调用，只把尺寸/格式写入影子；
/// glGetTexLevelParameter* 从影子回答。与 gl_state 同为线程局部语义。
struct ProxyState {
    width: i32,
    height: i32,
    intformat: u32,
}

thread_local! {
    static PROXY_STATE: RefCell<ProxyState> = RefCell::new(ProxyState { width: 0, height: 0, intformat: 0 });
}

fn set_proxy_width(value: i32) {
    PROXY_STATE.with(|s| s.borrow_mut().width = value);
}

fn set_proxy_height(value: i32) {
    PROXY_STATE.with(|s| s.borrow_mut().height = value);
}

fn set_proxy_intformat(value: u32) {
    PROXY_STATE.with(|s| s.borrow_mut().intformat = value);
}

/// 目标映射（MG mg.cpp map_tex_target）：1D/3D/RECTANGLE 在 GLES 中按 2D 语义
/// 处理，PROXY 变体统一落到 PROXY_TEXTURE_2D 影子。
fn map_tex_target(target: u32) -> u32 {
    match target {
        GL_TEXTURE_1D | GL_TEXTURE_3D | GL_TEXTURE_RECTANGLE => GL_TEXTURE_2D,
        GL_PROXY_TEXTURE_1D | GL_PROXY_TEXTURE_3D | GL_PROXY_TEXTURE_RECTANGLE => {
            GL_PROXY_TEXTURE_2D
        }
        _ => target,
    }
}

// ===========================================================================
// internal_convert：内部格式/类型/格式三元组转换（MG texture.cpp:494-815）
// ===========================================================================

/// 深度专用格式（MG texture.cpp:1118 的 is_depth_format）。
fn is_depth_format(format: u32) -> bool {
    matches!(
        format,
        GL_DEPTH_COMPONENT
            | GL_DEPTH_COMPONENT16
            | GL_DEPTH_COMPONENT24
            | GL_DEPTH_COMPONENT32
            | GL_DEPTH_COMPONENT32F
    )
}

/// 深度+模板组合格式（MG texture.cpp:1134 的 is_depth_stencil_format）。
fn is_depth_stencil_format(format: u32) -> bool {
    matches!(
        format,
        GL_DEPTH_STENCIL | GL_DEPTH24_STENCIL8 | GL_DEPTH32F_STENCIL8
    )
}

/// 上传是否携带字节（MG mg_upload_has_data：pixels 非空，或空指针但有 unpack PBO）。
fn upload_has_data(dispatch: &backend::dispatch::GlesDispatch, pixels: *const c_void) -> bool {
    if !pixels.is_null() {
        return true;
    }
    let mut pbo = 0i32;
    unsafe { (dispatch.get_integerv)(GL_PIXEL_UNPACK_BUFFER_BINDING, &mut pbo) };
    pbo != 0
}

/// 内部格式 → 像素传输 (format, type) 转换（MG internal_convert 移植）。
///
/// `type_`/`format` 为 None 表示该调用无此参数（glTexStorage*），此时不推导；
/// `has_data` 表示调用是否携带字节（glTexImage* 的 allocation-only 与 storage
/// 走不同的深度格式解析分支）。
#[allow(clippy::too_many_lines)]
fn internal_convert(
    dispatch: &backend::dispatch::GlesDispatch,
    internal_format: &mut u32,
    type_: Option<&mut u32>,
    format: Option<&mut u32>,
    has_data: bool,
) {
    // GL_BGRA 刻意不改名：改名只改枚举不改数据。像素传输入口先经 CPU 转换
    // （UploadFix）同时转换数据，再调用本函数。
    match *internal_format {
        GL_DEPTH_COMPONENT16 => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_SHORT;
            }
        }
        GL_DEPTH_COMPONENT24 => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_INT;
            }
        }
        GL_DEPTH_COMPONENT32 => {
            // 无 GLES 驱动接受 GL_DEPTH_COMPONENT32（Mali/Adreno 均无
            // GL_OES_depth32）。GL_DEPTH_COMPONENT24 保持名字承诺的 unorm 分布
            // （32F 不保持），且是深度 blit 唯一接受的 depth-only 形式。
            *internal_format = GL_DEPTH_COMPONENT24;
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_INT;
            }
        }
        GL_DEPTH_COMPONENT32F => {
            if let Some(t) = type_ {
                *t = GL_FLOAT;
            }
        }
        GL_DEPTH_COMPONENT => {
            if has_data {
                // 带字节的 glTexImage*：故意保留 unsized（ES2 兼容接受），
                // 但 GL_FLOAT 是兼容性覆盖不到的类型，必须升到 32F。
                if let Some(t) = type_ {
                    if *t == GL_FLOAT {
                        *internal_format = GL_DEPTH_COMPONENT32F;
                    } else {
                        *internal_format = GL_DEPTH_COMPONENT;
                        *t = GL_UNSIGNED_INT;
                    }
                }
            } else if type_.is_some() {
                // allocation-only glTexImage*：type 描述不了字节，不能让它决定
                // 存储类别（否则 GL_FLOAT 分配会让整张纹理变浮点，后续定点
                // 上传被 ES 拒绝）。
                *internal_format = GL_DEPTH_COMPONENT;
                if let Some(t) = type_ {
                    *t = GL_UNSIGNED_INT;
                }
            } else {
                // glTexStorage*：unsized 形式被两家驱动直接拒绝，且不分配任何
                // 存储，升级为 24。
                *internal_format = GL_DEPTH_COMPONENT24;
            }
        }
        GL_DEPTH_STENCIL => {
            if let Some(t) = type_ {
                if *t == GL_FLOAT_32_UNSIGNED_INT_24_8_REV {
                    *internal_format = GL_DEPTH32F_STENCIL8;
                } else {
                    *internal_format = GL_DEPTH24_STENCIL8;
                    *t = GL_UNSIGNED_INT_24_8;
                }
            } else {
                *internal_format = GL_DEPTH24_STENCIL8;
            }
        }
        GL_RGB10_A2 => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_INT_2_10_10_10_REV;
            }
        }
        GL_RGB5_A1 => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_SHORT_5_5_5_1;
            }
        }
        GL_COMPRESSED_RED_RGTC1 | GL_COMPRESSED_RG_RGTC2 => {
            log::debug!(
                "[FluorateGL] internal_convert: 0x{:04X} (RGTC) 不被 GLES 支持，保持原值透传",
                *internal_format
            );
        }
        GL_SRGB8 => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_BYTE;
            }
        }
        GL_RGBA32F | GL_RGB32F => {
            if let Some(t) = type_ {
                *t = GL_FLOAT;
            }
        }
        GL_RGB9_E5 => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_INT_5_9_9_9_REV;
            }
        }
        GL_R11F_G11F_B10F => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_INT_10F_11F_11F_REV;
            }
            if let Some(f) = format {
                *f = GL_RGB;
            }
        }
        GL_RGBA32UI | GL_RGB32UI => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_INT;
            }
        }
        GL_RGBA32I | GL_RGB32I => {
            if let Some(t) = type_ {
                *t = GL_INT;
            }
        }
        GL_RGBA16 => {
            if gles_norm16(dispatch) {
                if let Some(t) = type_ {
                    *t = GL_UNSIGNED_SHORT;
                }
            } else {
                *internal_format = GL_RGBA16F;
                if let Some(t) = type_ {
                    *t = GL_FLOAT;
                }
            }
        }
        GL_RGBA8 | GL_RGBA => {
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_BYTE;
            }
            if let Some(f) = format {
                *f = GL_RGBA;
            }
        }
        GL_RGBA16F => {
            if let Some(t) = type_ {
                *t = GL_HALF_FLOAT;
            }
        }
        GL_R16 => {
            if gles_norm16(dispatch) {
                if let Some(t) = type_ {
                    *t = GL_UNSIGNED_SHORT;
                }
            } else {
                *internal_format = GL_R16F;
                if let Some(t) = type_ {
                    *t = GL_FLOAT;
                }
            }
            if let Some(f) = format {
                *f = GL_RED;
            }
        }
        GL_RGB16 => {
            if gles_norm16(dispatch) {
                if let Some(t) = type_ {
                    *t = GL_UNSIGNED_SHORT;
                }
            } else {
                *internal_format = GL_RGB16F;
                if let Some(t) = type_ {
                    *t = GL_HALF_FLOAT;
                }
            }
            if let Some(f) = format {
                *f = GL_RGB;
            }
        }
        GL_RGB16F => {
            if let Some(t) = type_ {
                *t = GL_HALF_FLOAT;
            }
            if let Some(f) = format {
                *f = GL_RGB;
            }
        }
        GL_RG16 => {
            if gles_norm16(dispatch) {
                if let Some(t) = type_ {
                    *t = GL_UNSIGNED_SHORT;
                }
            } else {
                *internal_format = GL_RG16F;
                if let Some(t) = type_ {
                    *t = GL_HALF_FLOAT;
                }
            }
            if let Some(f) = format {
                *f = GL_RG;
            }
        }
        GL_R8 => {
            if let Some(f) = format {
                *f = GL_RED;
            }
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_BYTE;
            }
        }
        GL_R8_SNORM => {
            if let Some(f) = format {
                *f = GL_RED;
            }
            if let Some(t) = type_ {
                *t = GL_BYTE;
            }
        }
        GL_R16F => {
            if let Some(f) = format {
                *f = GL_RED;
            }
            if let Some(t) = type_ {
                *t = GL_HALF_FLOAT;
            }
        }
        GL_RED => {
            if let Some(t) = type_ {
                match *t {
                    GL_UNSIGNED_BYTE => {
                        *internal_format = GL_R8;
                        if let Some(f) = format {
                            *f = GL_RED;
                        }
                    }
                    GL_BYTE => {
                        *internal_format = GL_R8_SNORM;
                        if let Some(f) = format {
                            *f = GL_RED;
                        }
                    }
                    GL_HALF_FLOAT => {
                        *internal_format = GL_R16F;
                        if let Some(f) = format {
                            *f = GL_RED;
                        }
                    }
                    GL_FLOAT => {
                        *internal_format = GL_R32F;
                        if let Some(f) = format {
                            *f = GL_RED;
                        }
                    }
                    _ => {
                        log::debug!(
                            "[FluorateGL] internal_convert: GL_RED 不支持 type 0x{:04X}，回退 R8/UBYTE",
                            *t
                        );
                        *t = GL_UNSIGNED_BYTE;
                        *internal_format = GL_R8;
                        if let Some(f) = format {
                            *f = GL_RED;
                        }
                    }
                }
            }
        }
        GL_R8UI => {
            if let Some(f) = format {
                *f = GL_RED_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_BYTE;
            }
        }
        GL_R8I => {
            if let Some(f) = format {
                *f = GL_RED_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_BYTE;
            }
        }
        GL_R16UI => {
            if let Some(f) = format {
                *f = GL_RED_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_SHORT;
            }
        }
        GL_R16I => {
            if let Some(f) = format {
                *f = GL_RED_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_SHORT;
            }
        }
        GL_R32UI => {
            if let Some(f) = format {
                *f = GL_RED_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_INT;
            }
        }
        GL_R32I => {
            if let Some(f) = format {
                *f = GL_RED_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_INT;
            }
        }
        GL_RG8 => {
            if let Some(f) = format {
                *f = GL_RG;
            }
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_BYTE;
            }
        }
        GL_RG8_SNORM => {
            if let Some(f) = format {
                *f = GL_RG;
            }
            if let Some(t) = type_ {
                *t = GL_BYTE;
            }
        }
        GL_RG16F => {
            if let Some(f) = format {
                *f = GL_RG;
            }
            if let Some(t) = type_ {
                *t = GL_HALF_FLOAT;
            }
        }
        GL_RG32F => {
            if let Some(f) = format {
                *f = GL_RG;
            }
            if let Some(t) = type_ {
                *t = GL_FLOAT;
            }
        }
        GL_RG8UI => {
            if let Some(f) = format {
                *f = GL_RG_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_BYTE;
            }
        }
        GL_RG8I => {
            if let Some(f) = format {
                *f = GL_RG_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_BYTE;
            }
        }
        GL_RG16UI => {
            if let Some(f) = format {
                *f = GL_RG_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_SHORT;
            }
        }
        GL_RG16I => {
            if let Some(f) = format {
                *f = GL_RG_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_SHORT;
            }
        }
        GL_RG32UI => {
            if let Some(f) = format {
                *f = GL_RG_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_UNSIGNED_INT;
            }
        }
        GL_RG32I => {
            if let Some(f) = format {
                *f = GL_RG_INTEGER;
            }
            if let Some(t) = type_ {
                *t = GL_INT;
            }
        }
        GL_R32F => {
            if let Some(f) = format {
                *f = GL_RED;
            }
            if let Some(t) = type_ {
                *t = GL_FLOAT;
            }
        }
        GL_RGBA8_SNORM => {
            if let Some(f) = format {
                *f = GL_RGBA;
            }
            if let Some(t) = type_ {
                *t = GL_BYTE;
            }
        }
        GL_RGB => {
            // unsized GL_RGB 的经典桌面用法：glTexImage2D(GL_RGB, ..., GL_RGBA,
            // GL_UNSIGNED_BYTE, rgba) 合法，转换时丢弃 alpha。
            if let Some(f) = format {
                *f = GL_RGB;
            }
            if let Some(t) = type_ {
                if *t != GL_UNSIGNED_BYTE && *t != GL_UNSIGNED_SHORT_5_6_5 {
                    *t = GL_UNSIGNED_BYTE;
                }
            }
        }
        _ => {
            // 兜底：GL_RGB8 / GL_RGBA16_SNORM 等。
            if *internal_format == GL_RGB8 {
                if let Some(t) = type_ {
                    if *t != GL_UNSIGNED_BYTE {
                        *t = GL_UNSIGNED_BYTE;
                    }
                }
                if let Some(f) = format {
                    *f = GL_RGB;
                }
            } else if *internal_format == GL_RGBA16_SNORM {
                if let Some(t) = type_ {
                    if *t != GL_SHORT {
                        *t = GL_SHORT;
                    }
                }
            }
        }
    }
}

// ===========================================================================
// 上传 CPU 转换（MG gl/transfer.cpp mg_upload_fix_t 移植）
// ===========================================================================

// 每线程转换 scratch 缓冲（容量 16MiB 以上且不再需要时回收，防常驻大内存）。
thread_local! {
    static TRANSFER_SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

const SCRATCH_KEEP: usize = 16 << 20; // 16 MiB

fn scratch_release_if_large(used: usize) {
    TRANSFER_SCRATCH.with(|s| {
        let mut buf = s.borrow_mut();
        if buf.capacity() <= SCRATCH_KEEP {
            return;
        }
        if used * 4 > buf.capacity() {
            return;
        }
        *buf = Vec::new();
    });
}

/// width*height*depth*channels 溢出安全乘法（MG mg_checked_area）。
fn checked_area(width: i32, height: i32, depth: i32, channels: usize) -> Option<usize> {
    if width <= 0 || height <= 0 || depth <= 0 || channels == 0 {
        return None;
    }
    let k_max: usize = if usize::BITS >= 64 {
        1usize << 40
    } else {
        usize::MAX
    };
    let mut n = width as usize;
    for f in [height as usize, depth as usize, channels] {
        if f != 0 && n > k_max / f {
            return None;
        }
        n *= f;
    }
    Some(n)
}

/// 行宽按像素存储对齐取整（MG pixel.h widthalign）。
fn width_align(width: usize, align: usize) -> usize {
    if align <= 1 {
        return width;
    }
    if (align & (align - 1)) != 0 {
        return width;
    }
    (width + (align - 1)) & !(align - 1)
}

type Decoder = unsafe fn(*const u8, *mut u8);

/// 解码器：读一个源像素（内存序）写 RGBA/RGB 字节（MG transfer.cpp decoders）。
unsafe fn dec_bgra_u8(s: *const u8, d: *mut u8) {
    unsafe {
        *d.add(0) = *s.add(2);
        *d.add(1) = *s.add(1);
        *d.add(2) = *s.add(0);
        *d.add(3) = *s.add(3);
    }
}

unsafe fn dec_bgr_u8(s: *const u8, d: *mut u8) {
    unsafe {
        *d.add(0) = *s.add(2);
        *d.add(1) = *s.add(1);
        *d.add(2) = *s.add(0);
    }
}

/// BGRA 打包 8888：B=31..24 G=23..16 R=15..8 A=7..0
unsafe fn dec_bgra_8888(s: *const u8, d: *mut u8) {
    unsafe {
        let v = u32::from_ne_bytes(std::ptr::read_unaligned(s.cast::<[u8; 4]>()));
        *d.add(0) = ((v >> 8) & 0xff) as u8;
        *d.add(1) = ((v >> 16) & 0xff) as u8;
        *d.add(2) = ((v >> 24) & 0xff) as u8;
        *d.add(3) = (v & 0xff) as u8;
    }
}

/// BGRA 打包 8888_REV：B=7..0 G=15..8 R=23..16 A=31..24
unsafe fn dec_bgra_8888_rev(s: *const u8, d: *mut u8) {
    unsafe {
        let v = u32::from_ne_bytes(std::ptr::read_unaligned(s.cast::<[u8; 4]>()));
        *d.add(0) = ((v >> 16) & 0xff) as u8;
        *d.add(1) = ((v >> 8) & 0xff) as u8;
        *d.add(2) = (v & 0xff) as u8;
        *d.add(3) = ((v >> 24) & 0xff) as u8;
    }
}

/// RGBA 打包 8888：R=31..24 G=23..16 B=15..8 A=7..0
unsafe fn dec_rgba_8888(s: *const u8, d: *mut u8) {
    unsafe {
        let v = u32::from_ne_bytes(std::ptr::read_unaligned(s.cast::<[u8; 4]>()));
        *d.add(0) = ((v >> 24) & 0xff) as u8;
        *d.add(1) = ((v >> 16) & 0xff) as u8;
        *d.add(2) = ((v >> 8) & 0xff) as u8;
        *d.add(3) = (v & 0xff) as u8;
    }
}

/// RGBA 打包 8888_REV：R=7..0 G=15..8 B=23..16 A=31..24
unsafe fn dec_rgba_8888_rev(s: *const u8, d: *mut u8) {
    unsafe {
        let v = u32::from_ne_bytes(std::ptr::read_unaligned(s.cast::<[u8; 4]>()));
        *d.add(0) = (v & 0xff) as u8;
        *d.add(1) = ((v >> 8) & 0xff) as u8;
        *d.add(2) = ((v >> 16) & 0xff) as u8;
        *d.add(3) = ((v >> 24) & 0xff) as u8;
    }
}

/// BGRA 打包 1555_REV：B=4..0 G=9..5 R=14..10 A=15
unsafe fn dec_bgra_1555_rev(s: *const u8, d: *mut u8) {
    unsafe {
        let v = u16::from_ne_bytes(std::ptr::read_unaligned(s.cast::<[u8; 2]>()));
        let b = (v & 0x1f) as u8;
        let g = ((v >> 5) & 0x1f) as u8;
        let r = ((v >> 10) & 0x1f) as u8;
        *d.add(0) = (r << 3) | (r >> 2);
        *d.add(1) = (g << 3) | (g >> 2);
        *d.add(2) = (b << 3) | (b >> 2);
        *d.add(3) = if v & 0x8000 != 0 { 255 } else { 0 };
    }
}

/// BGRA 打包 4444_REV：B=3..0 G=7..4 R=11..8 A=15..12
unsafe fn dec_bgra_4444_rev(s: *const u8, d: *mut u8) {
    unsafe {
        let v = u16::from_ne_bytes(std::ptr::read_unaligned(s.cast::<[u8; 2]>()));
        *d.add(0) = ((v >> 8) & 0x0f) as u8 * 17;
        *d.add(1) = ((v >> 4) & 0x0f) as u8 * 17;
        *d.add(2) = (v & 0x0f) as u8 * 17;
        *d.add(3) = ((v >> 12) & 0x0f) as u8 * 17;
    }
}

/// 直拷贝：仅当目标通道数不同时选中（want_format 适配）。
unsafe fn dec_rgb_u8(s: *const u8, d: *mut u8) {
    unsafe {
        *d.add(0) = *s.add(0);
        *d.add(1) = *s.add(1);
        *d.add(2) = *s.add(2);
    }
}

unsafe fn dec_rgba_u8(s: *const u8, d: *mut u8) {
    unsafe {
        std::ptr::copy_nonoverlapping(s, d, 4);
    }
}

struct UploadRule {
    format: u32,
    type_: u32,
    dec: Decoder,
    src_size: usize,
    channels: usize,
    /// 仅用于目标通道数适配，不单独选中
    adapt_only: bool,
}

static UPLOAD_RULES: &[UploadRule] = &[
    UploadRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_BYTE,
        dec: dec_bgra_u8,
        src_size: 4,
        channels: 4,
        adapt_only: false,
    },
    UploadRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_INT_8_8_8_8_REV,
        dec: dec_bgra_8888_rev,
        src_size: 4,
        channels: 4,
        adapt_only: false,
    },
    UploadRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_INT_8_8_8_8,
        dec: dec_bgra_8888,
        src_size: 4,
        channels: 4,
        adapt_only: false,
    },
    UploadRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_SHORT_1_5_5_5_REV,
        dec: dec_bgra_1555_rev,
        src_size: 2,
        channels: 4,
        adapt_only: false,
    },
    UploadRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_SHORT_4_4_4_4_REV,
        dec: dec_bgra_4444_rev,
        src_size: 2,
        channels: 4,
        adapt_only: false,
    },
    UploadRule {
        format: GL_BGR,
        type_: GL_UNSIGNED_BYTE,
        dec: dec_bgr_u8,
        src_size: 3,
        channels: 3,
        adapt_only: false,
    },
    // 打包 8888 类型本身在 GLES 不存在（无 REV 也一样）。
    UploadRule {
        format: GL_RGBA,
        type_: GL_UNSIGNED_INT_8_8_8_8_REV,
        dec: dec_rgba_8888_rev,
        src_size: 4,
        channels: 4,
        adapt_only: false,
    },
    UploadRule {
        format: GL_RGBA,
        type_: GL_UNSIGNED_INT_8_8_8_8,
        dec: dec_rgba_8888,
        src_size: 4,
        channels: 4,
        adapt_only: false,
    },
    UploadRule {
        format: GL_RGB,
        type_: GL_UNSIGNED_BYTE,
        dec: dec_rgb_u8,
        src_size: 3,
        channels: 3,
        adapt_only: true,
    },
    UploadRule {
        format: GL_RGBA,
        type_: GL_UNSIGNED_BYTE,
        dec: dec_rgba_u8,
        src_size: 4,
        channels: 4,
        adapt_only: true,
    },
];

fn channels_of(format: u32) -> usize {
    match format {
        GL_RGBA | GL_BGRA => 4,
        GL_RGB | GL_BGR => 3,
        _ => 0,
    }
}

fn find_upload_rule(format: u32, type_: u32, want_format: u32) -> Option<&'static UploadRule> {
    let mut adapt = None;
    for r in UPLOAD_RULES {
        if r.format != format || r.type_ != type_ {
            continue;
        }
        if !r.adapt_only {
            return Some(r);
        }
        adapt = Some(r);
    }
    // adapt-only 规则仅在调用方指明目标格式且通道数与源不同时启用。
    if let Some(adapt) = adapt {
        if want_format != 0 {
            let want = channels_of(want_format);
            if want != 0 && want != adapt.channels {
                return Some(adapt);
            }
        }
    }
    None
}

/// 一次上传的 (format, type, pixels) 修复（MG mg_upload_fix_t）。
///
/// 对 GLES 无法接受的客户端格式组合做 CPU 重排；转换后输出紧凑流，故驱动侧
/// unpack 参数被清零，Drop 恢复所有被触碰的状态（含 unpack PBO 绑定）。
struct UploadFix<'a> {
    dispatch: &'a backend::dispatch::GlesDispatch,
    format: u32,
    type_: u32,
    pixels: *const c_void,
    /// 调用是否携带字节（allocation-only glTexImage 无字节可描述）
    has_data: bool,
    /// 转换无法执行，调用方不得发起传输
    dropped: bool,
    converted: bool,
    pbo_unbound: bool,
    prev_pbo: i32,
    prev_align: i32,
    prev_row_len: i32,
    prev_skip_rows: i32,
    prev_skip_px: i32,
    converted_bytes: usize,
    copy_scratch: u32,
    prev_copy_write: i32,
}

impl<'a> UploadFix<'a> {
    /// 转换无法执行（调用方不得发起传输）
    fn dropped(&self) -> bool {
        self.dropped
    }

    /// 调用是否携带字节（allocation-only glTexImage 无字节可描述）
    fn has_data(&self) -> bool {
        self.has_data
    }

    /// `want_format`：目标客户端格式（internal_convert 后的 format），非 0 时
    /// 转换按该格式的通道数输出。`three_d`：3D 传输（GL_UNPACK_IMAGE_HEIGHT /
    /// SKIP_IMAGES 仅描述三维传输；本层无 unpack 镜像，恒按 0 处理并告警一次）。
    #[allow(clippy::too_many_arguments)]
    fn new(
        dispatch: &'a backend::dispatch::GlesDispatch,
        width: i32,
        height: i32,
        depth: i32,
        format_in: u32,
        type_in: u32,
        pixels_in: *const c_void,
        want_format: u32,
        three_d: bool,
    ) -> Self {
        let mut fix = Self {
            dispatch,
            format: format_in,
            type_: type_in,
            pixels: pixels_in,
            has_data: false,
            dropped: false,
            converted: false,
            pbo_unbound: false,
            prev_pbo: 0,
            prev_align: 4,
            prev_row_len: 0,
            prev_skip_rows: 0,
            prev_skip_px: 0,
            converted_bytes: 0,
            copy_scratch: 0,
            prev_copy_write: 0,
        };

        let Some(rule) = find_upload_rule(format_in, type_in, want_format) else {
            return fix;
        };

        // 输出通道数：目标要求多少就输出多少，枚举必须描述真实字节。
        let mut out_channels = rule.channels;
        if want_format != 0 {
            let want = channels_of(want_format);
            if want != 0 {
                out_channels = want;
            }
        }
        fix.format = if out_channels == 4 { GL_RGBA } else { GL_RGB };
        fix.type_ = GL_UNSIGNED_BYTE;

        // PBO 绑定查询（驱动状态；本层 glPixelStorei 纯透传，驱动值即真实值）。
        let mut pbo = 0i32;
        unsafe { (dispatch.get_integerv)(GL_PIXEL_UNPACK_BUFFER_BINDING, &mut pbo) };
        fix.prev_pbo = pbo;
        fix.has_data = !(pixels_in.is_null() && pbo == 0);
        if !fix.has_data {
            return fix; // allocation-only：只需修正枚举
        }
        if width <= 0 || height <= 0 || depth <= 0 {
            return fix;
        }

        // unpack 参数：GLES 有的直接查询驱动；GL_UNPACK_IMAGE_HEIGHT /
        // SKIP_IMAGES 在 GLES 无对应 pname，本层无镜像 → 按 0（MG 用 gl_state
        // 镜像；协调点见 glPixelStorei 域）。
        unsafe {
            (dispatch.get_integerv)(GL_UNPACK_ALIGNMENT, &mut fix.prev_align);
            (dispatch.get_integerv)(GL_UNPACK_ROW_LENGTH, &mut fix.prev_row_len);
            (dispatch.get_integerv)(GL_UNPACK_SKIP_ROWS, &mut fix.prev_skip_rows);
            (dispatch.get_integerv)(GL_UNPACK_SKIP_PIXELS, &mut fix.prev_skip_px);
        }
        if three_d && !UNPACK_IMAGE_HEIGHT_UNTRACKED_WARNED.swap(true, Ordering::Relaxed) {
            log::warn!(
                "[FluorateGL] 3D 像素传输的 GL_UNPACK_IMAGE_HEIGHT/SKIP_IMAGES 未被跟踪（GLES 无此 pname，本层无 unpack 镜像），按 0 处理 (后续调用将静默)"
            );
        }
        // MG 用镜像的 eff_img_h/eff_skip_img；这里恒为 0。
        let eff_img_h: usize = 0;
        let eff_skip_img: usize = 0;

        let ss = rule.src_size;
        let row_px = if fix.prev_row_len > 0 {
            fix.prev_row_len as usize
        } else {
            width as usize
        };
        let row_stride = width_align(row_px * ss, fix.prev_align as usize);
        let img_rows = if eff_img_h > 0 {
            eff_img_h
        } else {
            height as usize
        };
        let img_stride = row_stride * img_rows;
        let start = eff_skip_img * img_stride
            + fix.prev_skip_rows as usize * row_stride
            + fix.prev_skip_px as usize * ss;
        let span = start
            + (depth as usize - 1) * img_stride
            + (height as usize - 1) * row_stride
            + width as usize * ss;

        // 源解析：客户端指针，或 unpack PBO（map 直读 → copy scratch 兜底）。
        let mut src: *const u8 = std::ptr::null();
        let mut mapped: *mut c_void = std::ptr::null_mut();
        let mut mapped_target = GL_PIXEL_UNPACK_BUFFER;
        if pbo != 0 {
            let pbo_off = pixels_in as usize as isize;
            mapped = unsafe {
                (dispatch.map_buffer_range)(
                    GL_PIXEL_UNPACK_BUFFER,
                    pbo_off,
                    span as isize,
                    GL_MAP_READ_BIT,
                )
            };
            if !mapped.is_null() {
                src = mapped as *const u8;
            } else {
                // 移动驱动常拒绝 _DRAW 用法 hint 缓冲的 READ 映射，走拷贝。
                // 先清掉失败 map 留下的错误。
                unsafe { while (dispatch.get_error)() != 0 {} }
                let mut copy_scratch = 0u32;
                let mut prev_copy_write = 0i32;
                unsafe {
                    (dispatch.get_integerv)(GL_COPY_WRITE_BUFFER_BINDING, &mut prev_copy_write);
                    (dispatch.gen_buffers)(1, &mut copy_scratch);
                    (dispatch.bind_buffer)(GL_COPY_WRITE_BUFFER, copy_scratch);
                    (dispatch.buffer_data)(
                        GL_COPY_WRITE_BUFFER,
                        span as isize,
                        std::ptr::null(),
                        GL_STREAM_READ,
                    );
                    (dispatch.copy_buffer_sub_data)(
                        GL_PIXEL_UNPACK_BUFFER,
                        GL_COPY_WRITE_BUFFER,
                        pbo_off,
                        0,
                        span as isize,
                    );
                    mapped = (dispatch.map_buffer_range)(
                        GL_COPY_WRITE_BUFFER,
                        0,
                        span as isize,
                        GL_MAP_READ_BIT,
                    );
                }
                if !mapped.is_null() {
                    fix.copy_scratch = copy_scratch;
                    fix.prev_copy_write = prev_copy_write;
                    mapped_target = GL_COPY_WRITE_BUFFER;
                    src = mapped as *const u8;
                } else {
                    unsafe {
                        (dispatch.bind_buffer)(GL_COPY_WRITE_BUFFER, prev_copy_write as u32);
                        (dispatch.delete_buffers)(1, &copy_scratch);
                    }
                }
            }
            if src.is_null() {
                if !UPLOAD_PBO_UNREADABLE_WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!(
                        "[FluorateGL] 像素传输: unpack buffer {} 既不能映射也不能拷贝读取，上传已丢弃 (后续调用将静默)",
                        pbo
                    );
                }
                unsafe { (dispatch.bind_buffer)(GL_PIXEL_UNPACK_BUFFER, 0) };
                fix.pbo_unbound = true;
                fix.pixels = std::ptr::null();
                fix.dropped = true;
                return fix;
            }
        } else {
            src = pixels_in as *const u8;
        }

        // 尺寸检查：先于 resize，防止 bad_alloc 或解码越界。
        let Some(need) = checked_area(width, height, depth, out_channels) else {
            if !UPLOAD_SIZE_OVERFLOW_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "[FluorateGL] 像素传输: {}x{}x{} x {} 通道超出内存，已丢弃 (后续调用将静默)",
                    width,
                    height,
                    depth,
                    out_channels
                );
            }
            unsafe {
                Self::release_source_unsafe(
                    dispatch,
                    mapped,
                    mapped_target,
                    fix.copy_scratch,
                    fix.prev_copy_write,
                );
            }
            fix.pbo_unbound = true;
            fix.pixels = std::ptr::null();
            fix.dropped = true;
            return fix;
        };

        // GL_UNPACK_SWAP_BYTES：GLES 无此 pname 且本层无镜像 → 恒 false，
        // 不做字节交换（协调点见 glPixelStorei 域）。

        let out_ptr = TRANSFER_SCRATCH.with(|s| {
            let mut buf = s.borrow_mut();
            buf.resize(need, 0);
            let ptr = buf.as_mut_ptr();
            fix.converted_bytes = need;
            ptr
        });

        unsafe {
            let mut out = out_ptr;
            for z in 0..depth as usize {
                for row in 0..height as usize {
                    let s = src.add(start + z * img_stride + row * row_stride);
                    for col in 0..width as usize {
                        let mut px = [0u8, 0, 0, 255];
                        (rule.dec)(s.add(col * ss), px.as_mut_ptr());
                        for c in 0..out_channels {
                            *out.add(c) = px[c];
                        }
                        out = out.add(out_channels);
                    }
                }
            }
        }

        // 释放源读取通道并解绑 unpack PBO：驱动调用时的像素必须是客户端内存
        // （scratch），若 PBO 仍绑定会把 scratch 指针误读为 PBO 偏移（MG
        // release_source 同语义，绑定由 Drop 恢复）。
        unsafe {
            Self::release_source_unsafe(
                dispatch,
                mapped,
                mapped_target,
                fix.copy_scratch,
                fix.prev_copy_write,
            );
        }
        fix.pbo_unbound = true;

        // 转换流紧凑且 skips 已应用：清零驱动侧 unpack 参数。
        unsafe {
            (dispatch.pixel_store_i)(GL_UNPACK_ALIGNMENT, 1);
            (dispatch.pixel_store_i)(GL_UNPACK_ROW_LENGTH, 0);
            (dispatch.pixel_store_i)(GL_UNPACK_SKIP_ROWS, 0);
            (dispatch.pixel_store_i)(GL_UNPACK_SKIP_PIXELS, 0);
        }
        fix.converted = true;
        fix.pixels = out_ptr as *const c_void;
        fix
    }

    /// 释放 PBO 源（unmap + 解绑 unpack PBO），MG release_source。
    #[allow(clippy::too_many_arguments)]
    unsafe fn release_source_unsafe(
        dispatch: &backend::dispatch::GlesDispatch,
        mapped: *mut c_void,
        mapped_target: u32,
        copy_scratch: u32,
        prev_copy_write: i32,
    ) {
        unsafe {
            if mapped.is_null() {
                return;
            }
            if copy_scratch != 0 {
                (dispatch.unmap_buffer)(GL_COPY_WRITE_BUFFER);
                (dispatch.bind_buffer)(GL_COPY_WRITE_BUFFER, prev_copy_write as u32);
                (dispatch.delete_buffers)(1, &copy_scratch);
            } else {
                (dispatch.unmap_buffer)(mapped_target);
            }
            // 转换期间解绑 unpack PBO：驱动调用（glTexImage*/SubImage*）的像素
            // 必须是客户端内存（scratch），绑定恢复由 Drop 完成。
            (dispatch.bind_buffer)(GL_PIXEL_UNPACK_BUFFER, 0);
        }
    }
}

impl Drop for UploadFix<'_> {
    fn drop(&mut self) {
        let dispatch = self.dispatch;
        if self.converted {
            unsafe {
                (dispatch.pixel_store_i)(GL_UNPACK_ALIGNMENT, self.prev_align);
                (dispatch.pixel_store_i)(GL_UNPACK_ROW_LENGTH, self.prev_row_len);
                (dispatch.pixel_store_i)(GL_UNPACK_SKIP_ROWS, self.prev_skip_rows);
                (dispatch.pixel_store_i)(GL_UNPACK_SKIP_PIXELS, self.prev_skip_px);
            }
        }
        if self.pbo_unbound {
            unsafe {
                (dispatch.bind_buffer)(GL_PIXEL_UNPACK_BUFFER, self.prev_pbo as u32);
            }
        }
        if self.converted {
            scratch_release_if_large(self.converted_bytes);
        }
    }
}

// ===========================================================================
// 读回（glGetTexImage 的 FBO 模拟 + RGBA→目标格式编码，MG 移植）
// ===========================================================================

type Encoder = unsafe fn(*const u8, *mut u8);

/// 编码器：RGBA 内存序字节进，一个目标像素出（MG transfer.cpp encoders）。
unsafe fn enc_bgra_u8(s: *const u8, d: *mut u8) {
    unsafe {
        *d.add(0) = *s.add(2);
        *d.add(1) = *s.add(1);
        *d.add(2) = *s.add(0);
        *d.add(3) = *s.add(3);
    }
}

unsafe fn enc_rgb_u8(s: *const u8, d: *mut u8) {
    unsafe {
        *d.add(0) = *s.add(0);
        *d.add(1) = *s.add(1);
        *d.add(2) = *s.add(2);
    }
}

unsafe fn enc_rg_u8(s: *const u8, d: *mut u8) {
    unsafe {
        *d.add(0) = *s.add(0);
        *d.add(1) = *s.add(1);
    }
}

unsafe fn enc_r_u8(s: *const u8, d: *mut u8) {
    unsafe {
        *d.add(0) = *s.add(0);
    }
}

unsafe fn enc_bgr_u8(s: *const u8, d: *mut u8) {
    unsafe {
        *d.add(0) = *s.add(2);
        *d.add(1) = *s.add(1);
        *d.add(2) = *s.add(0);
    }
}

unsafe fn enc_bgra_8888(s: *const u8, d: *mut u8) {
    unsafe {
        let v = (u32::from(*s.add(2)) << 24)
            | (u32::from(*s.add(1)) << 16)
            | (u32::from(*s.add(0)) << 8)
            | u32::from(*s.add(3));
        std::ptr::write_unaligned(d.cast::<u32>(), v);
    }
}

unsafe fn enc_bgra_8888_rev(s: *const u8, d: *mut u8) {
    unsafe {
        let v = u32::from(*s.add(2))
            | (u32::from(*s.add(1)) << 8)
            | (u32::from(*s.add(0)) << 16)
            | (u32::from(*s.add(3)) << 24);
        std::ptr::write_unaligned(d.cast::<u32>(), v);
    }
}

unsafe fn enc_rgba_8888(s: *const u8, d: *mut u8) {
    unsafe {
        let v = (u32::from(*s.add(0)) << 24)
            | (u32::from(*s.add(1)) << 16)
            | (u32::from(*s.add(2)) << 8)
            | u32::from(*s.add(3));
        std::ptr::write_unaligned(d.cast::<u32>(), v);
    }
}

unsafe fn enc_rgba_8888_rev(s: *const u8, d: *mut u8) {
    unsafe {
        let v = u32::from(*s.add(0))
            | (u32::from(*s.add(1)) << 8)
            | (u32::from(*s.add(2)) << 16)
            | (u32::from(*s.add(3)) << 24);
        std::ptr::write_unaligned(d.cast::<u32>(), v);
    }
}

/// 1555_REV 编码：截断即解码的逆（5 位 31 → 255 → 31）。
unsafe fn enc_bgra_1555_rev(s: *const u8, d: *mut u8) {
    unsafe {
        let v = (u16::from(*s.add(2)) >> 3)
            | (u16::from(*s.add(1)) >> 3) << 5
            | (u16::from(*s.add(0)) >> 3) << 10
            | if *s.add(3) >= 128 { 0x8000 } else { 0 };
        std::ptr::write_unaligned(d.cast::<u16>(), v);
    }
}

/// 4444_REV 编码。
unsafe fn enc_bgra_4444_rev(s: *const u8, d: *mut u8) {
    unsafe {
        let v = (u16::from(*s.add(2)) >> 4)
            | (u16::from(*s.add(1)) >> 4) << 4
            | (u16::from(*s.add(0)) >> 4) << 8
            | (u16::from(*s.add(3)) >> 4) << 12;
        std::ptr::write_unaligned(d.cast::<u16>(), v);
    }
}

struct ReadbackRule {
    format: u32,
    type_: u32,
    enc: Encoder,
    dst_size: usize,
}

static READBACK_RULES: &[ReadbackRule] = &[
    ReadbackRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_BYTE,
        enc: enc_bgra_u8,
        dst_size: 4,
    },
    ReadbackRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_INT_8_8_8_8_REV,
        enc: enc_bgra_8888_rev,
        dst_size: 4,
    },
    ReadbackRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_INT_8_8_8_8,
        enc: enc_bgra_8888,
        dst_size: 4,
    },
    ReadbackRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_SHORT_1_5_5_5_REV,
        enc: enc_bgra_1555_rev,
        dst_size: 2,
    },
    ReadbackRule {
        format: GL_BGRA,
        type_: GL_UNSIGNED_SHORT_4_4_4_4_REV,
        enc: enc_bgra_4444_rev,
        dst_size: 2,
    },
    ReadbackRule {
        format: GL_BGR,
        type_: GL_UNSIGNED_BYTE,
        enc: enc_bgr_u8,
        dst_size: 3,
    },
    ReadbackRule {
        format: GL_RGB,
        type_: GL_UNSIGNED_BYTE,
        enc: enc_rgb_u8,
        dst_size: 3,
    },
    ReadbackRule {
        format: GL_RG,
        type_: GL_UNSIGNED_BYTE,
        enc: enc_rg_u8,
        dst_size: 2,
    },
    ReadbackRule {
        format: GL_RED,
        type_: GL_UNSIGNED_BYTE,
        enc: enc_r_u8,
        dst_size: 1,
    },
    ReadbackRule {
        format: GL_RGBA,
        type_: GL_UNSIGNED_INT_8_8_8_8_REV,
        enc: enc_rgba_8888_rev,
        dst_size: 4,
    },
    ReadbackRule {
        format: GL_RGBA,
        type_: GL_UNSIGNED_INT_8_8_8_8,
        enc: enc_rgba_8888,
        dst_size: 4,
    },
];

/// (format, type) 能否在此后端读回（MG mg_readback_pair_supported）。
///
/// 规则表可重编码 → 可交付；GL_RGBA/UNSIGNED_BYTE 恒可；其余问
/// GL_IMPLEMENTATION_COLOR_READ_FORMAT/TYPE（读 FBO 的附件决定，不缓存）。
fn readback_pair_supported(
    dispatch: &backend::dispatch::GlesDispatch,
    format: u32,
    type_: u32,
) -> bool {
    if READBACK_RULES
        .iter()
        .any(|r| r.format == format && r.type_ == type_)
    {
        return true;
    }
    if format == GL_RGBA && type_ == GL_UNSIGNED_BYTE {
        return true;
    }
    let mut impl_format = 0i32;
    let mut impl_type = 0i32;
    unsafe {
        (dispatch.get_integerv)(GL_IMPLEMENTATION_COLOR_READ_FORMAT, &mut impl_format);
        (dispatch.get_integerv)(GL_IMPLEMENTATION_COLOR_READ_TYPE, &mut impl_type);
    }
    impl_format as u32 == format && impl_type as u32 == type_
}

/// 执行一次需要重编码的读回（MG mg_transfer_readback）。返回 true 表示已处理。
///
/// 以紧凑 RGBA 读入 scratch（pack 状态与 PBO 暂时移开），再按应用的 pack 状态
/// 编码到目标（支持 pack PBO：map 写入，失败退行式 glBufferSubData）。
fn transfer_readback(
    dispatch: &backend::dispatch::GlesDispatch,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    format: u32,
    type_: u32,
    pixels: *mut c_void,
) -> bool {
    let Some(rule) = READBACK_RULES
        .iter()
        .find(|r| r.format == format && r.type_ == type_)
    else {
        return false;
    };
    if width <= 0 || height <= 0 {
        return true; // 我们的组合，且无事可做
    }

    let mut pbo = 0i32;
    let mut align = 4i32;
    let mut row_len = 0i32;
    let mut skip_rows = 0i32;
    let mut skip_px = 0i32;
    unsafe {
        (dispatch.get_integerv)(GL_PIXEL_PACK_BUFFER_BINDING, &mut pbo);
        (dispatch.get_integerv)(GL_PACK_ALIGNMENT, &mut align);
        (dispatch.get_integerv)(GL_PACK_ROW_LENGTH, &mut row_len);
        (dispatch.get_integerv)(GL_PACK_SKIP_ROWS, &mut skip_rows);
        (dispatch.get_integerv)(GL_PACK_SKIP_PIXELS, &mut skip_px);
    }

    let scratch_len = (width as usize) * (height as usize) * 4;
    let scratch_ptr = TRANSFER_SCRATCH.with(|s| {
        let mut buf = s.borrow_mut();
        buf.resize(scratch_len, 0);
        buf.as_mut_ptr()
    });

    // 紧凑 RGBA 读入 scratch，pack 状态与 PBO 移开。
    unsafe {
        if pbo != 0 {
            (dispatch.bind_buffer)(GL_PIXEL_PACK_BUFFER, 0);
        }
        (dispatch.pixel_store_i)(GL_PACK_ALIGNMENT, 1);
        (dispatch.pixel_store_i)(GL_PACK_ROW_LENGTH, 0);
        (dispatch.pixel_store_i)(GL_PACK_SKIP_ROWS, 0);
        (dispatch.pixel_store_i)(GL_PACK_SKIP_PIXELS, 0);
        // 先排空队列，避免应用遗留错误被读成我们自己的失败。
        while (dispatch.get_error)() != 0 {}
        (dispatch.read_pixels)(
            x,
            y,
            width,
            height,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            scratch_ptr as *mut c_void,
        );
        let read_err = (dispatch.get_error)();
        (dispatch.pixel_store_i)(GL_PACK_ALIGNMENT, align);
        (dispatch.pixel_store_i)(GL_PACK_ROW_LENGTH, row_len);
        (dispatch.pixel_store_i)(GL_PACK_SKIP_ROWS, skip_rows);
        (dispatch.pixel_store_i)(GL_PACK_SKIP_PIXELS, skip_px);
        if read_err != 0 {
            // 不是每张读缓冲都以 RGBA8 交回。交给驱动：pair 可能是该缓冲的
            // 实现定义读格式，不是则驱动拒绝且不碰目标。
            if !READBACK_RGBA_FAILED_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "[FluorateGL] 像素读回: RGBA 读回失败 (0x{:04X})，将 {} + {} 交回驱动 (后续调用将静默)",
                    read_err,
                    format,
                    type_
                );
            }
            if pbo != 0 {
                (dispatch.bind_buffer)(GL_PIXEL_PACK_BUFFER, pbo as u32);
            }
            return false;
        }
    }

    // 按应用 pack 状态编码到目标。
    let ds = rule.dst_size;
    let row_px = if row_len > 0 {
        row_len as usize
    } else {
        width as usize
    };
    let dst_stride = width_align(row_px * ds, align as usize);
    let start = skip_rows as usize * dst_stride + skip_px as usize * ds;
    let span = start + (height as usize - 1) * dst_stride + width as usize * ds;
    let pbo_offset = pixels as usize as isize;
    let row_bytes = width as usize * ds;

    let mut dst: *mut u8 = std::ptr::null_mut();
    let mut mapped: *mut c_void = std::ptr::null_mut();
    let mut via_subdata = false;
    let mut row_buf: Vec<u8> = Vec::new();

    unsafe {
        if pbo != 0 {
            (dispatch.bind_buffer)(GL_PIXEL_PACK_BUFFER, pbo as u32);
            mapped = (dispatch.map_buffer_range)(
                GL_PIXEL_PACK_BUFFER,
                pbo_offset,
                span as isize,
                GL_MAP_WRITE_BIT,
            );
            if mapped.is_null() {
                if !READBACK_PBO_UNWRITABLE_WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!(
                        "[FluorateGL] 像素读回: pack buffer {} 不可写映射，改用 glBufferSubData 行式写入 (后续调用将静默)",
                        pbo
                    );
                }
                row_buf.resize(row_bytes, 0);
                via_subdata = true;
            } else {
                dst = mapped as *mut u8;
            }
        } else {
            if pixels.is_null() {
                return true;
            }
            dst = pixels as *mut u8;
        }
    }

    // GL_PACK_SWAP_BYTES：GLES 无此 pname 且本层无镜像 → 恒 false，不做字节交换。
    let src = scratch_ptr;
    unsafe {
        if via_subdata {
            while (dispatch.get_error)() != 0 {}
        }
        for row in 0..height as usize {
            let row_off = start + row * dst_stride;
            let d = if via_subdata {
                row_buf.as_mut_ptr()
            } else {
                dst.add(row_off)
            };
            for col in 0..width as usize {
                (rule.enc)(src.add((row * width as usize + col) * 4), d.add(col * ds));
            }
            if via_subdata {
                (dispatch.buffer_sub_data)(
                    GL_PIXEL_PACK_BUFFER,
                    pbo_offset + row_off as isize,
                    row_bytes as isize,
                    row_buf.as_ptr() as *const c_void,
                );
                if row == 0 {
                    // 不可变、不可映射、不动态：该缓冲完全无法写入。
                    let sub_err = (dispatch.get_error)();
                    if sub_err != 0 {
                        if !READBACK_PBO_UNWRITABLE_WARNED.swap(true, Ordering::Relaxed) {
                            log::warn!(
                                "[FluorateGL] 像素读回: pack buffer {} 既不能写映射也不能 glBufferSubData (0x{:04X})，读回已丢弃 (后续调用将静默)",
                                pbo,
                                sub_err
                            );
                        }
                        return true;
                    }
                }
            }
        }
        if !mapped.is_null() {
            (dispatch.unmap_buffer)(GL_PIXEL_PACK_BUFFER);
        }
    }
    true
}

// ===========================================================================
// 内部格式归一化（保留：storage 的 legacy 兜底 + compressed 降级路径）
// ===========================================================================

/// Convert a desktop OpenGL internal format to the closest GLES-compatible
/// internal format.
///
/// 注意：glTexImage*/glTexStorage* 主路径已由 [`internal_convert`] 处理（含
/// norm16 能力判断、深度 type 推导）；本函数仅作为 unsized→sized 的 legacy
/// 兜底（MG 的 internal_convert 不覆盖 unsized color 格式，这里保留我们的
/// 稳健映射）以及压缩纹理降级路径使用。
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
        // RGB16/RGBA16 在 GLES 3.x 中不存在（无 GL_EXT_texture_norm16 时），
        // 映射到半浮点版本。internal_convert 主路径会按 norm16 能力更精确地
        // 处理；此处为兜底（compressed 降级路径等无 dispatch 的场景）。
        GL_RGB16 => GL_RGB16F,
        GL_RGBA16 => GL_RGBA16F,
        GL_RGBA2 | GL_RGBA4 | GL_RGBA12 => GL_RGBA8,
        GL_BGR => GL_RGB8,
        GL_BGRA => GL_RGBA8,
        // MG 语义：GL_DEPTH_COMPONENT32 降级为 24（无 GL_OES_depth32，且 24
        // 保持 unorm 分布、是深度 blit 唯一接受的 depth-only 形式）。
        GL_DEPTH_COMPONENT32 => GL_DEPTH_COMPONENT24,
        GL_STENCIL_INDEX8 | GL_STENCIL_INDEX16 => GL_R8,

        // 4. 桌面 GL 的"让驱动自选压缩格式"标志，GLES 不支持，降级为 sized 格式
        GL_COMPRESSED_RGBA => GL_RGBA8,
        GL_COMPRESSED_RGB => GL_RGB8,

        _ => internalformat,
    }
}

// ===========================================================================
// 导出函数
// ===========================================================================

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
            // 影子对象表与绑定表同步清理（MG MarkTextureObjectForDeletion 的
            // 影子部分；绑定槽的驱动侧清零由 GLES 自己完成）。
            meta_remove(desktop_id);
            BOUND_TEXTURES.with(|m| {
                m.borrow_mut().retain(|_, v| *v != desktop_id);
            });
            if let Some(gles_id) = state::with_state(|s| s.textures.delete(desktop_id)) {
                (dispatch.delete_textures)(1, &gles_id);
            }
        }
    });
}

/// texture buffer emulate 借用单元（MG texture.cpp:290 MG_TEXTURE_BUFFER_EMULATION_UNIT）。
///
/// glBindTexture(GL_TEXTURE_BUFFER) 与 glTexBuffer 把模拟的缓冲纹理停放在该
/// 单元的 GL_TEXTURE_2D 上；应用侧活跃单元在借用前后恢复。buffer 域（glTexBuffer
/// 行式上传）与 drawing 域（samplerBuffer 采样 uniform）共用此约定。
pub(crate) const MG_TEXTURE_BUFFER_EMULATION_UNIT: i32 = 15;

/// texture buffer emulate 开关（MG loader.cpp set_hardware：GLES <= 3.1 时开启）。
///
/// 由 buffer 域在能力探测后调用 [`set_texture_buffer_emulation`] 设置；
/// texture.rs 的 glBindTexture 据此把 GL_TEXTURE_BUFFER 绑定转挂到借用单元。
pub(crate) static TEXTURE_BUFFER_EMULATION: AtomicBool = AtomicBool::new(false);

#[allow(dead_code)]
/// 设置 texture buffer emulate 开关（buffer 域能力探测后调用）。
pub(crate) fn set_texture_buffer_emulation(enabled: bool) {
    TEXTURE_BUFFER_EMULATION.store(enabled, Ordering::Relaxed);
}

#[allow(dead_code)]
/// 读取 texture buffer emulate 开关。
pub(crate) fn texture_buffer_emulation_enabled() -> bool {
    TEXTURE_BUFFER_EMULATION.load(Ordering::Relaxed)
}

#[allow(dead_code)] // 跨域接口预留（buffer 域 glTexBuffer 模拟对接，当前无调用方）
#[allow(private_bounds)] // TextureMeta 出现在泛型 bound——接口冻结保留
/// 更新纹理影子元数据（buffer 域 glTexBuffer 模拟成功后记录 internal_format/
/// 尺寸/swizzle，MG buffer.cpp:927-936）。
pub(crate) fn update_texture_meta<F: FnOnce(&mut TextureMeta)>(id: u32, f: F) {
    meta_get_mut(id, f);
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
                    warn_texture_id_miss("glBindTexture", target, texture);
                    0
                })
            })
        };

        // texture buffer emulate（MG texture.cpp:1495-1543）：GLES <= 3.1 时
        // GL_TEXTURE_BUFFER 的绑定停放到借用单元（15）的 GL_TEXTURE_2D 上，
        // 应用侧活跃单元在借用前后恢复。
        if texture_buffer_emulation_enabled() && target == GL_TEXTURE_BUFFER {
            let mut current_unit = 0i32;
            (dispatch.get_integerv)(GL_ACTIVE_TEXTURE, &mut current_unit);
            (dispatch.active_texture)(GL_TEXTURE0 + MG_TEXTURE_BUFFER_EMULATION_UNIT as u32);
            (dispatch.bind_texture)(GL_TEXTURE_2D, gles_id);
            (dispatch.active_texture)(current_unit as u32);
        } else {
            (dispatch.bind_texture)(target, gles_id);
        }
        state::with_state(|s| s.bound_texture = texture);
        // 影子绑定表 + 对象 target 记录（MG GetOrCreateTextureObject + slot Bind）。
        if texture != 0 && gles_id != 0 {
            BOUND_TEXTURES.with(|m| {
                m.borrow_mut().insert(normalize_target(target), texture);
            });
            meta_get_mut(texture, |meta| meta.target = normalize_target(target));
        }
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        // MG 顺序：先 internal_convert（重写目标格式/类型/内部格式），后 CPU
        // 转换数据（按目标格式输出通道数）。转换被拒则调用已报错，不得再定义 level。
        let mut want_if = internalformat as u32;
        let mut want_fmt = format;
        let mut want_type = type_;
        internal_convert(
            dispatch,
            &mut want_if,
            Some(&mut want_type),
            Some(&mut want_fmt),
            upload_has_data(dispatch, pixels),
        );
        let internalformat = want_if as i32;

        let fix = UploadFix::new(
            dispatch, width, height, 1, format, type_, pixels, want_fmt, false,
        );
        if fix.dropped() {
            return;
        }
        let (fmt, typ) = if fix.has_data() {
            (fix.format, fix.type_)
        } else {
            (want_fmt, want_type)
        };

        // PROXY 查询（MG texture.cpp:924-932）：纯 CPU 模拟，不发 GLES 调用。
        if map_tex_target(target) == GL_PROXY_TEXTURE_2D {
            let mut max_size = 4096i32;
            (dispatch.get_integerv)(GL_MAX_TEXTURE_SIZE, &mut max_size);
            let shift = level.clamp(0, 31) as u32;
            set_proxy_width(if width.wrapping_shl(shift) > max_size {
                0
            } else {
                width
            });
            set_proxy_height(if height.wrapping_shl(shift) > max_size {
                0
            } else {
                height
            });
            set_proxy_intformat(want_if);
            return;
        }

        // 影子对象表（MG GET_TEXTURE_OBJECT）：记录最近分配的属性，swizzle 重置。
        if let Some(desktop_id) = bound_texture_for_target(target) {
            meta_get_mut(desktop_id, |meta| {
                meta.target = normalize_target(target);
                meta.internal_format = want_if;
                meta.format = fmt;
                meta.width = width;
                meta.height = height;
                meta.depth = 1;
                meta.swizzle = [
                    GL_RED as i32,
                    GL_GREEN as i32,
                    GL_BLUE as i32,
                    GL_ALPHA as i32,
                ];
            });
        }

        (dispatch.tex_image_2d)(
            target,
            level,
            internalformat,
            width,
            height,
            border,
            fmt,
            typ,
            fix.pixels,
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
    // 排查日志：记录像素数据上传元数据（不 dump 像素内容，量太大）
    log::debug!(
        "[FluorateGL] glTexSubImage2D(target=0x{:04X} level={} offset=({},{}) size=({}x{}) format=0x{:04X} type=0x{:04X} pixels={:p})",
        target,
        level,
        xoffset,
        yoffset,
        width,
        height,
        format,
        type_,
        pixels
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        // MG：子上传同样要转换数据（旧的"改 swizzle 兜底"是采样状态副作用，
        // 已被 CPU 转换取代）。转换被拒则上传不得发出（glTexSubImage2D 没有
        // 只分配形式，null 像素 + 无 unpack buffer 会读地址零）。
        let fix = UploadFix::new(dispatch, width, height, 1, format, type_, pixels, 0, false);
        if fix.dropped() {
            return;
        }
        (dispatch.tex_sub_image_2d)(
            target, level, xoffset, yoffset, width, height, fix.format, fix.type_, fix.pixels,
        );
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        let mut want_if = internalformat as u32;
        let mut want_fmt = format;
        let mut want_type = type_;
        internal_convert(
            dispatch,
            &mut want_if,
            Some(&mut want_type),
            Some(&mut want_fmt),
            upload_has_data(dispatch, pixels),
        );
        let normalized_if = want_if as i32;
        if normalized_if != internalformat {
            log::debug!(
                "glTexImage3D: normalized internalformat 0x{:04X} -> 0x{:04X}",
                internalformat,
                normalized_if
            );
        }

        let fix = UploadFix::new(
            dispatch, width, height, depth, format, type_, pixels, want_fmt, true,
        );
        if fix.dropped() {
            return;
        }
        let (fmt, typ) = if fix.has_data() {
            (fix.format, fix.type_)
        } else {
            (want_fmt, want_type)
        };

        // PROXY 查询（MG texture.cpp:981-990）：3D 目标经 map_tex_target 统一
        // 落到 PROXY_TEXTURE_2D 影子（depth 不跟踪，MG 注释掉了该行）。
        if map_tex_target(target) == GL_PROXY_TEXTURE_2D {
            let mut max_size = 4096i32;
            (dispatch.get_integerv)(GL_MAX_TEXTURE_SIZE, &mut max_size);
            let shift = level.clamp(0, 31) as u32;
            set_proxy_width(if width.wrapping_shl(shift) > max_size {
                0
            } else {
                width
            });
            set_proxy_height(if height.wrapping_shl(shift) > max_size {
                0
            } else {
                height
            });
            set_proxy_intformat(want_if);
            return;
        }

        if let Some(desktop_id) = bound_texture_for_target(target) {
            meta_get_mut(desktop_id, |meta| {
                meta.target = normalize_target(target);
                meta.internal_format = want_if;
                meta.format = fmt;
                meta.width = width;
                meta.height = height;
                meta.depth = depth;
                meta.swizzle = [
                    GL_RED as i32,
                    GL_GREEN as i32,
                    GL_BLUE as i32,
                    GL_ALPHA as i32,
                ];
            });
        }

        (dispatch.tex_image_3d)(
            target,
            level,
            normalized_if,
            width,
            height,
            depth,
            border,
            fmt,
            typ,
            fix.pixels,
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
        let fix = UploadFix::new(
            dispatch, width, height, depth, format, type_, pixels, 0, true,
        );
        if fix.dropped() {
            return;
        }
        (dispatch.tex_sub_image_3d)(
            target, level, xoffset, yoffset, zoffset, width, height, depth, fix.format, fix.type_,
            fix.pixels,
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
    log::debug!(
        "[FluorateGL] glTexStorage2D(target=0x{:04X}, levels={}, internalformat=0x{:04X}, {}x{})",
        target,
        levels,
        internalformat,
        width,
        height
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        // MG 降级路径：internal_convert 无 data/type（glTexStorage 只有
        // internalformat），深度类按 storage 分支选择 sized；随后保留我们的
        // unsized→sized legacy 兜底。
        let mut normalized = internalformat;
        internal_convert(dispatch, &mut normalized, None, None, false);
        let normalized = normalize_internal_format(normalized);
        if normalized != internalformat {
            log::debug!(
                "[FluorateGL] glTexStorage2D: normalized internalformat 0x{:04X} -> 0x{:04X}",
                internalformat,
                normalized
            );
        }

        if let Some(desktop_id) = bound_texture_for_target(target) {
            meta_get_mut(desktop_id, |meta| {
                meta.target = normalize_target(target);
                meta.internal_format = normalized;
                meta.width = width;
                meta.height = height;
                meta.depth = 1;
                meta.swizzle = [
                    GL_RED as i32,
                    GL_GREEN as i32,
                    GL_BLUE as i32,
                    GL_ALPHA as i32,
                ];
            });
        }

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
    backend::with_gles_dispatch(|dispatch| unsafe {
        let mut normalized = internalformat;
        internal_convert(dispatch, &mut normalized, None, None, false);
        let normalized = normalize_internal_format(normalized);
        if normalized != internalformat {
            log::debug!(
                "glTexStorage3D: normalized internalformat 0x{:04X} -> 0x{:04X}",
                internalformat,
                normalized
            );
        }

        if let Some(desktop_id) = bound_texture_for_target(target) {
            meta_get_mut(desktop_id, |meta| {
                meta.target = normalize_target(target);
                meta.internal_format = normalized;
                meta.width = width;
                meta.height = height;
                meta.depth = depth;
                meta.swizzle = [
                    GL_RED as i32,
                    GL_GREEN as i32,
                    GL_BLUE as i32,
                    GL_ALPHA as i32,
                ];
            });
        }

        (dispatch.tex_storage_3d)(target, levels, normalized, width, height, depth);
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        // MG pname_convert：桌面 GL_TEXTURE_LOD_BIAS → QCOM 变体（仅当驱动
        // 支持 GL_QCOM_texture_lod_bias；不支持则跳过，与 MG 一致）。
        let mut pname = pname;
        if pname == GL_TEXTURE_LOD_BIAS {
            if !gles_lod_bias_qcom(dispatch) {
                log::debug!(
                    "[FluorateGL] glTexParameteri pname 0x{:04X} skipped (驱动不支持 GL_QCOM_texture_lod_bias)",
                    pname
                );
                return;
            }
            pname = GL_TEXTURE_LOD_BIAS_QCOM;
        }
        (dispatch.tex_parameter_i)(target, pname, param);
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        let mut pname = pname;
        if pname == GL_TEXTURE_LOD_BIAS {
            if !gles_lod_bias_qcom(dispatch) {
                log::debug!(
                    "[FluorateGL] glTexParameterf pname 0x{:04X} skipped (驱动不支持 GL_QCOM_texture_lod_bias)",
                    pname
                );
                return;
            }
            pname = GL_TEXTURE_LOD_BIAS_QCOM;
        }
        (dispatch.tex_parameter_f)(target, pname, param);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameterfv(target: u32, pname: u32, params: *const f32) {
    // MG 语义：fv 为原生透传（不做 LOD_BIAS 转换），GLES 无 GL_TEXTURE_LOD_BIAS
    // pname，透传会得到驱动 INVALID_ENUM——与 MG 一致。
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.tex_parameter_fv)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexParameteriv(target: u32, pname: u32, params: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if pname == GL_TEXTURE_SWIZZLE_RGBA {
            // MG：GLES 无 GL_TEXTURE_SWIZZLE_RGBA 单个 pname，展开为 R/G/B/A
            // 四次调用，并记录影子状态（MG texture.cpp:1430）。
            if params.is_null() {
                log::warn!(
                    "[FluorateGL] glTexParameteriv: params 为 null (GL_TEXTURE_SWIZZLE_RGBA)"
                );
                return;
            }
            (dispatch.tex_parameter_i)(target, GL_TEXTURE_SWIZZLE_R, *params);
            (dispatch.tex_parameter_i)(target, GL_TEXTURE_SWIZZLE_G, *params.add(1));
            (dispatch.tex_parameter_i)(target, GL_TEXTURE_SWIZZLE_B, *params.add(2));
            (dispatch.tex_parameter_i)(target, GL_TEXTURE_SWIZZLE_A, *params.add(3));

            if let Some(desktop_id) = bound_texture_for_target(target) {
                meta_get_mut(desktop_id, |meta| {
                    meta.swizzle = [*params, *params.add(1), *params.add(2), *params.add(3)];
                });
            }
        } else {
            (dispatch.tex_parameter_iv)(target, pname, params);
        }
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

    // S3TC 不是 GLES core 压缩格式：先按驱动能力列表判断，不支持则忽略该上传
    // （不传坏数据，避免 INVALID_ENUM 污染错误队列）。
    if (0x83F0..=0x83F3).contains(&internalformat) {
        let supported =
            backend::with_gles_dispatch(|d| gles_supports_compressed_format(d, internalformat));
        if !supported {
            if !S3TC_UNSUPPORTED_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "[FluorateGL] glCompressedTexImage2D: S3TC internalformat 0x{:04X} 不被 GLES 驱动支持，忽略该上传 (后续调用将静默跳过)",
                    internalformat
                );
            }
            return;
        }
    }

    // 防止将非压缩格式透传给 GLES 导致 GL_INVALID_ENUM 崩溃
    if !is_compressed_format(internalformat) {
        let normalized = normalize_internal_format(internalformat);
        warn_compressed_format_mismatch("glCompressedTexImage2D", internalformat, normalized);
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
        warn_compressed_format_mismatch(
            "glCompressedTexImage3D",
            internalformat,
            normalize_internal_format(internalformat),
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
            // GLES 没有 glGetTexImage，用临时 FBO + glReadPixels 模拟（MG
            // texture.cpp:1619 移植）。
            emulate_get_tex_image(dispatch, target, level, format, type_, pixels);
            return;
        }
        (dispatch.get_tex_image)(target, level, format, type_, pixels);
    });
}

/// 用临时 FBO + glReadPixels 模拟 glGetTexImage（MG texture.cpp:1619 移植）。
///
/// 流程：
/// 1. 保存 READ/DRAW FBO 绑定；创建临时 FBO；
/// 2. 查询当前绑定到 target 的 GLES 纹理 ID 及该 level 宽高；
/// 3. 纹理该 level 挂到 COLOR_ATTACHMENT0，检查 FBO 完整性；
/// 4. glReadBuffer(GL_COLOR_ATTACHMENT0) 后读回像素；
/// 5. 删除临时 FBO，恢复 READ/DRAW FBO 绑定。
///
/// 与旧实现的差异（对齐 MG）：
/// - 分别保存/恢复 READ_FRAMEBUFFER_BINDING 与 DRAW_FRAMEBUFFER_BINDING；
/// - 读回前检查 (format, type) pair 支持（readback 规则表可重编码，或
///   RGBA/UNSIGNED_BYTE，或实现定义读格式），不支持时告警并跳过，不再把
///   未初始化的缓冲当作成功读回；
/// - 规则表命中的 pair（GL_RGB/GL_RED/GL_BGRA/8888 packed 等）走
///   transfer_readback 的 RGBA 读回 + CPU 编码，支持 pack PBO 与 pack 状态。
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
        // 1. 解析 target → 绑定查询 pname（仅 2D 与 cube 面可模拟；3D/2D_ARRAY
        //    无 layer 参数无法精确模拟，MG 同样拒绝并报错）。
        let binding_pname = if target == GL_TEXTURE_2D {
            GL_TEXTURE_BINDING_2D
        } else if (GL_TEXTURE_CUBE_MAP_POSITIVE_X..=GL_TEXTURE_CUBE_MAP_NEGATIVE_Z)
            .contains(&target)
        {
            GL_TEXTURE_BINDING_CUBE_MAP
        } else {
            log::warn!(
                "[FluorateGL] glGetTexImage: 不支持的 target 0x{:04X}（3D/array/其他无法以颜色附件读回），已跳过",
                target
            );
            return;
        };

        // 2. 保存当前 READ/DRAW FBO 绑定
        let mut prev_read_fbo = 0i32;
        let mut prev_draw_fbo = 0i32;
        (dispatch.get_integerv)(GL_READ_FRAMEBUFFER_BINDING, &mut prev_read_fbo);
        (dispatch.get_integerv)(GL_DRAW_FRAMEBUFFER_BINDING, &mut prev_draw_fbo);

        // 3. 创建临时 FBO
        let mut fbo = 0u32;
        (dispatch.gen_framebuffers)(1, &mut fbo);
        (dispatch.bind_framebuffer)(GL_FRAMEBUFFER, fbo);

        // 4. 查询绑定纹理
        let mut tex = 0i32;
        (dispatch.get_integerv)(binding_pname, &mut tex);
        if tex <= 0 {
            log::warn!(
                "[FluorateGL] glGetTexImage: target 0x{:04X} 当前无绑定纹理，已跳过",
                target
            );
            (dispatch.bind_framebuffer)(GL_READ_FRAMEBUFFER, prev_read_fbo as u32);
            (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
            (dispatch.delete_framebuffers)(1, &fbo);
            return;
        }

        // 5. 查询该 level 宽高
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
            (dispatch.bind_framebuffer)(GL_READ_FRAMEBUFFER, prev_read_fbo as u32);
            (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
            (dispatch.delete_framebuffers)(1, &fbo);
            return;
        }

        // 6. 挂载纹理该 level 到颜色附件
        (dispatch.framebuffer_texture_2d)(
            GL_FRAMEBUFFER,
            GL_COLOR_ATTACHMENT0,
            target,
            tex as u32,
            level,
        );
        let status = (dispatch.check_framebuffer_status)(GL_FRAMEBUFFER);
        if status != GL_FRAMEBUFFER_COMPLETE {
            // 绝大多数是深度/深度模板 level：非颜色可渲染，挂 COLOR_ATTACHMENT0
            // 永不完整。旧实现这里静默返回。
            log::warn!(
                "[FluorateGL] glGetTexImage: FBO 不完整 (status=0x{:04X})，无法读回纹理（可能是深度纹理），已跳过",
                status
            );
            (dispatch.bind_framebuffer)(GL_READ_FRAMEBUFFER, prev_read_fbo as u32);
            (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
            (dispatch.delete_framebuffers)(1, &fbo);
            return;
        }

        // 7. 读回 pair 支持检查（在临时 FBO 绑定后查询：实现定义读格式按
        //    读帧缓冲的附件选择）。
        if !readback_pair_supported(dispatch, format, type_) {
            if !READBACK_PAIR_UNSUPPORTED_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "[FluorateGL] glGetTexImage: format 0x{:04X} + type 0x{:04X} 无法在此后端读回，已跳过 (后续调用将静默)",
                    format,
                    type_
                );
            }
            (dispatch.bind_framebuffer)(GL_READ_FRAMEBUFFER, prev_read_fbo as u32);
            (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
            (dispatch.delete_framebuffers)(1, &fbo);
            return;
        }

        // 8. 读回：规则表命中的 pair 由 transfer_readback 做 RGBA 读回 + CPU
        //    编码；其余（RGBA/UBYTE 或实现定义读格式）直读驱动。
        (dispatch.read_buffer)(GL_COLOR_ATTACHMENT0);
        if !transfer_readback(dispatch, 0, 0, width, height, format, type_, pixels) {
            (dispatch.read_pixels)(0, 0, width, height, format, type_, pixels);
        }

        // 9. 清理并恢复（READ_BUFFER 随 FBO 绑定恢复，MG 不单独恢复）
        (dispatch.bind_framebuffer)(GL_READ_FRAMEBUFFER, prev_read_fbo as u32);
        (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
        (dispatch.delete_framebuffers)(1, &fbo);

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
    // PROXY 查询从影子回答（MG texture.cpp:1402-1428）；其余透传。
    if map_tex_target(target) == GL_PROXY_TEXTURE_2D {
        PROXY_STATE.with(|s| {
            let st = s.borrow();
            let value = match pname {
                GL_TEXTURE_WIDTH => nlevel(st.width, level),
                GL_TEXTURE_HEIGHT => nlevel(st.height, level),
                GL_TEXTURE_INTERNAL_FORMAT => st.intformat as i32,
                _ => return, // MG 语义：其余 pname 不写、直接返回
            };
            unsafe { *params = value };
        });
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

/// glClearTexImage（GL_ARB_clear_texture）——MG texture.cpp:1881 的真实实现移植。
///
/// GL 4.6 sec. 8.15：null data 清零，非 null 用该值清除。实现：临时 FBO +
/// glClear（按格式选附件与 mask），cube map 逐面、3D/array 逐层；清除值保存
/// 在 clear 状态守卫中，任何退出路径都恢复应用的 clear 值。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearTexImage(
    texture: u32,
    level: i32,
    format: u32,
    type_: u32,
    data: *const std::ffi::c_void,
) {
    log::debug!(
        "[FluorateGL] glClearTexImage(texture={}, level={}, format=0x{:04X}, type=0x{:04X}, data={:?})",
        texture,
        level,
        format,
        type_,
        data
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        // ID 翻译（与 glFramebufferTexture2D 一致）：desktop → GLES。
        let gles_texture = if texture == 0 {
            0
        } else {
            state::with_state(|s| {
                s.textures.get_gles(texture).unwrap_or_else(|| {
                    warn_texture_id_miss("glClearTexImage", GL_TEXTURE_2D, texture);
                    0
                })
            })
        };
        if gles_texture == 0 {
            return;
        }

        // 解析清除值：null 清零，否则按 format/type 解码；读不了则丢弃
        // （用错误值清除是"看似正确"的错误图像）。
        let mut rgba = [0f32, 0f32, 0f32, 0f32];
        let mut depth = 0f32;
        let mut stencil = 0i32;
        if !data.is_null()
            && !decode_clear_value(format, type_, data, &mut rgba, &mut depth, &mut stencil)
        {
            if !CLEAR_VALUE_UNDECODABLE_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "[FluorateGL] glClearTexImage: 无法解析 format 0x{:04X} + type 0x{:04X} 的清除值，清除已丢弃 (后续调用将静默)",
                    format,
                    type_
                );
            }
            return;
        }

        // 附件与 mask 按格式选择：深度/模板纹理挂 COLOR_ATTACHMENT0 永不完整。
        let (attachment, mask) = if format == GL_DEPTH_COMPONENT {
            (GL_DEPTH_ATTACHMENT, GL_DEPTH_BUFFER_BIT)
        } else if format == GL_STENCIL_INDEX {
            (GL_STENCIL_ATTACHMENT, GL_STENCIL_BUFFER_BIT)
        } else if format == GL_DEPTH_STENCIL {
            (
                GL_DEPTH_STENCIL_ATTACHMENT,
                GL_DEPTH_BUFFER_BIT | GL_STENCIL_BUFFER_BIT,
            )
        } else {
            (GL_COLOR_ATTACHMENT0, GL_COLOR_BUFFER_BIT)
        };

        let mut prev_draw_fbo = 0i32;
        let mut prev_read_fbo = 0i32;
        (dispatch.get_integerv)(GL_DRAW_FRAMEBUFFER_BINDING, &mut prev_draw_fbo);
        (dispatch.get_integerv)(GL_READ_FRAMEBUFFER_BINDING, &mut prev_read_fbo);

        let mut fbo = 0u32;
        (dispatch.gen_framebuffers)(1, &mut fbo);
        (dispatch.bind_framebuffer)(GL_FRAMEBUFFER, fbo);

        // 清除值守卫：恢复应用的 clear 状态（MG clear_state_guard）。
        let mut prev_color = [0f32; 4];
        let mut prev_depth = 1f32;
        let mut prev_stencil = 0i32;
        (dispatch.get_float_v)(GL_COLOR_CLEAR_VALUE, prev_color.as_mut_ptr());
        (dispatch.get_float_v)(GL_DEPTH_CLEAR_VALUE, &mut prev_depth);
        (dispatch.get_integerv)(GL_STENCIL_CLEAR_VALUE, &mut prev_stencil);
        (dispatch.clear_color)(rgba[0], rgba[1], rgba[2], rgba[3]);
        (dispatch.clear_depth)(depth);
        (dispatch.clear_stencil)(stencil);

        // 纹理形状判定（MG mgGetTexObjectByID → tex->target）：cube 六面、
        // 3D/array 逐层，其余按 2D。
        let meta = meta_get(texture);
        let texture_target = meta.target;

        // 闭包：挂载并清除一层；返回是否完整。
        let attach_and_clear = |dispatch: &backend::dispatch::GlesDispatch,
                                attachment: u32,
                                face: u32,
                                layer: i32|
         -> bool {
            if layer < 0 {
                (dispatch.framebuffer_texture_2d)(
                    GL_FRAMEBUFFER,
                    attachment,
                    face,
                    gles_texture,
                    level,
                );
            } else {
                (dispatch.framebuffer_texture_layer)(
                    GL_FRAMEBUFFER,
                    attachment,
                    gles_texture,
                    level,
                    layer,
                );
            }
            if (dispatch.check_framebuffer_status)(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
                return false;
            }
            (dispatch.clear)(mask);
            true
        };

        let mut cleared = true;
        if texture_target == GL_TEXTURE_CUBE_MAP {
            let mut face = GL_TEXTURE_CUBE_MAP_POSITIVE_X;
            while cleared && face <= GL_TEXTURE_CUBE_MAP_NEGATIVE_Z {
                cleared = attach_and_clear(dispatch, attachment, face, -1);
                face += 1;
            }
        } else if texture_target == GL_TEXTURE_3D || texture_target == GL_TEXTURE_2D_ARRAY {
            // 层数必须是本 level 的：先问驱动（GL_TEXTURE_DEPTH），失败回退
            // 影子 depth（glTexStorage3D 记录一次；数组无逐级收缩）。
            let mut layers = 0i32;
            (dispatch.get_tex_level_parameter_iv)(
                texture_target,
                level,
                GL_TEXTURE_DEPTH,
                &mut layers,
            );
            if layers <= 0 {
                layers = nlevel(
                    meta.depth,
                    if texture_target == GL_TEXTURE_3D {
                        level
                    } else {
                        0
                    },
                );
            }
            if layers <= 0 {
                cleared = false;
            } else {
                for layer in 0..layers {
                    if !attach_and_clear(dispatch, attachment, 0, layer) {
                        cleared = false;
                        break;
                    }
                }
            }
        } else {
            cleared = attach_and_clear(dispatch, attachment, GL_TEXTURE_2D, -1);
        }

        if !cleared && !CLEAR_ATTACH_FAILED_WARNED.swap(true, Ordering::Relaxed) {
            log::warn!(
                "[FluorateGL] glClearTexImage: texture {} (target 0x{:04X}) level {} 作为 format 0x{:04X} 无法挂载到帧缓冲，未清除任何内容 (后续调用将静默)",
                texture,
                texture_target,
                level,
                format
            );
        }

        // 恢复 clear 状态 + FBO 绑定，删除临时 FBO。
        (dispatch.clear_color)(prev_color[0], prev_color[1], prev_color[2], prev_color[3]);
        (dispatch.clear_depth)(prev_depth);
        (dispatch.clear_stencil)(prev_stencil);
        (dispatch.bind_framebuffer)(GL_READ_FRAMEBUFFER, prev_read_fbo as u32);
        (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
        (dispatch.delete_framebuffers)(1, &fbo);
    });
}

/// level 收缩（MG texture.cpp:48 的 nlevel）：size >> level，缩到 0 取 1。
fn nlevel(size: i32, level: i32) -> i32 {
    if size != 0 {
        let mut s = size >> level;
        if s == 0 {
            s = 1;
        }
        s
    } else {
        0
    }
}

/// 读取 glClearTexImage 收到的一个 texel 到三个清除寄存器（MG texture.cpp:1806
/// decode_clear_value 移植）。返回 false 表示该组合无法读取。
fn decode_clear_value(
    format: u32,
    type_: u32,
    data: *const c_void,
    rgba: &mut [f32; 4],
    depth: &mut f32,
    stencil: &mut i32,
) -> bool {
    match format {
        GL_RGBA | GL_RGB | GL_BGRA | GL_BGR => {
            let has_alpha = format == GL_RGBA || format == GL_BGRA;
            let reversed = format == GL_BGRA || format == GL_BGR;
            if type_ == GL_UNSIGNED_BYTE {
                let b = data as *const u8;
                rgba[0] = unsafe { *b.add(if reversed { 2 } else { 0 }) } as f32 / 255.0;
                rgba[1] = unsafe { *b.add(1) } as f32 / 255.0;
                rgba[2] = unsafe { *b.add(if reversed { 0 } else { 2 }) } as f32 / 255.0;
                rgba[3] = if has_alpha {
                    (unsafe { *b.add(3) }) as f32 / 255.0
                } else {
                    1.0
                };
                return true;
            }
            if type_ == GL_FLOAT {
                let f = data as *const f32;
                rgba[0] = unsafe { *f.add(if reversed { 2 } else { 0 }) };
                rgba[1] = unsafe { *f.add(1) };
                rgba[2] = unsafe { *f.add(if reversed { 0 } else { 2 }) };
                rgba[3] = if has_alpha { unsafe { *f.add(3) } } else { 1.0 };
                return true;
            }
            false
        }
        GL_DEPTH_COMPONENT => {
            if type_ == GL_FLOAT {
                *depth = unsafe { *(data as *const f32) };
                return true;
            }
            if type_ == GL_UNSIGNED_SHORT {
                *depth = unsafe { *(data as *const u16) } as f32 / 65535.0;
                return true;
            }
            if type_ == GL_UNSIGNED_INT {
                *depth = (unsafe { *(data as *const u32) } as f64 / 4294967295.0) as f32;
                return true;
            }
            false
        }
        GL_STENCIL_INDEX => {
            if type_ == GL_UNSIGNED_BYTE {
                *stencil = unsafe { *(data as *const u8) } as i32;
                return true;
            }
            if type_ == GL_UNSIGNED_INT {
                *stencil = unsafe { *(data as *const u32) } as i32;
                return true;
            }
            false
        }
        GL_DEPTH_STENCIL => {
            if type_ == GL_UNSIGNED_INT_24_8 {
                let v = unsafe { std::ptr::read_unaligned(data.cast::<u32>()) };
                *depth = ((v >> 8) as f64 / 16777215.0) as f32;
                *stencil = (v & 0xff) as i32;
                return true;
            }
            if type_ == GL_FLOAT_32_UNSIGNED_INT_24_8_REV {
                let d = unsafe { *(data as *const f32) };
                let s =
                    unsafe { std::ptr::read_unaligned((data as *const u8).add(4).cast::<u32>()) };
                *depth = d;
                *stencil = (s & 0xff) as i32;
                return true;
            }
            false
        }
        _ => false,
    }
}

/// glClearTexSubImage（GL_ARB_clear_texture 扩展，no-op stub）。
///
/// GLES 不支持此函数，MC 极少使用，no-op 安全（MG 同为 stub）。
/// 已声明 GL_ARB_clear_texture 扩展，必须导出避免 LWJGL capabilities 为 null。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearTexSubImage(
    texture: u32,
    level: i32,
    xoffset: i32,
    yoffset: i32,
    zoffset: i32,
    width: i32,
    height: i32,
    depth: i32,
    format: u32,
    type_: u32,
    data: *const std::ffi::c_void,
) {
    log::debug!(
        "[FluorateGL] glClearTexSubImage(texture={}, level={}, offset=[{},{},{}], size={}x{}x{}, format=0x{:04X}, type=0x{:04X}, data={:?}) -> no-op (GLES 不支持)",
        texture,
        level,
        xoffset,
        yoffset,
        zoffset,
        width,
        height,
        depth,
        format,
        type_,
        data
    );
}

/// glTexImage2DMultisample（GL 3.2，no-op stub）。
///
/// GLES 用 glTexStorage2DMultisample 替代，但 MC 极少使用 multisample texture，
/// no-op 安全（MG 同为 stub）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexImage2DMultisample(
    target: u32,
    samples: i32,
    internalformat: u32,
    width: i32,
    height: i32,
    fixedsamplelocations: u8,
) {
    log::debug!(
        "[FluorateGL] glTexImage2DMultisample(target=0x{:04X}, samples={}, internalformat=0x{:04X}, {}x{}, fixedsamplelocations={}) -> no-op (GLES 不支持)",
        target,
        samples,
        internalformat,
        width,
        height,
        fixedsamplelocations
    );
}

/// glTexImage3DMultisample（GL 3.2，no-op stub）。
///
/// GLES 用 glTexStorage3DMultisample 替代，但 MC 极少使用 multisample texture，
/// no-op 安全（MG 同为 stub）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexImage3DMultisample(
    target: u32,
    samples: i32,
    internalformat: u32,
    width: i32,
    height: i32,
    depth: i32,
    fixedsamplelocations: u8,
) {
    log::debug!(
        "[FluorateGL] glTexImage3DMultisample(target=0x{:04X}, samples={}, internalformat=0x{:04X}, {}x{}x{}, fixedsamplelocations={}) -> no-op (GLES 不支持)",
        target,
        samples,
        internalformat,
        width,
        height,
        depth,
        fixedsamplelocations
    );
}

/// glFramebufferTexture1D（GL 3.0，no-op stub）。
///
/// GLES 无 1D 纹理，no-op 安全（MG 同为 stub）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFramebufferTexture1D(
    target: u32,
    attachment: u32,
    textarget: u32,
    texture: u32,
    level: i32,
) {
    log::debug!(
        "[FluorateGL] glFramebufferTexture1D(target=0x{:04X}, attachment=0x{:04X}, textarget=0x{:04X}, texture={}, level={}) -> no-op (GLES 无 1D 纹理)",
        target,
        attachment,
        textarget,
        texture,
        level
    );
}

/// glFramebufferTexture3D（GL 3.0，转发 glFramebufferTextureLayer）。
///
/// GLES 无 glFramebufferTexture3D，但其语义等价于 glFramebufferTextureLayer
/// （texture 的 zoffset 层挂到 attachment），故直接转发（MG 为 stub，此处保留
/// 我们的转发增强）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFramebufferTexture3D(
    target: u32,
    attachment: u32,
    textarget: u32,
    texture: u32,
    level: i32,
    zoffset: i32,
) {
    log::debug!(
        "[FluorateGL] glFramebufferTexture3D(target=0x{:04X}, attachment=0x{:04X}, textarget=0x{:04X}, texture={}, level={}, zoffset={}) -> 转发 glFramebufferTextureLayer",
        target,
        attachment,
        textarget,
        texture,
        level,
        zoffset
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 纹理 ID 需从 desktop 翻译为 GLES（与 glFramebufferTexture2D/Layer 一致）。
        let gles_texture = if texture == 0 {
            0
        } else {
            state::with_state(|s| {
                s.textures.get_gles(texture).unwrap_or_else(|| {
                    warn_texture_id_miss("glFramebufferTexture3D", target, texture);
                    0
                })
            })
        };
        (dispatch.framebuffer_texture_layer)(target, attachment, gles_texture, level, zoffset);
    });
}

// ==== 纹理拷贝（GL 1.x-2.0 core 补齐，深度路径按 MG 语义模拟）====

/// target → 绑定查询 pname（MG texture.cpp:1145 get_binding_for_target）。
fn get_binding_for_target(target: u32) -> u32 {
    if target == GL_TEXTURE_2D {
        GL_TEXTURE_BINDING_2D
    } else if (GL_TEXTURE_CUBE_MAP_POSITIVE_X..=GL_TEXTURE_CUBE_MAP_NEGATIVE_Z).contains(&target) {
        GL_TEXTURE_BINDING_CUBE_MAP
    } else {
        0
    }
}

/// glCopyTexImage2D — GL 1.1（MG texture.cpp:1161 移植）。
///
/// 深度 internalformat 在 GLES 没有 glCopyTexImage2D 对应语义：先以
/// glTexImage2D 分配 level，再用临时 DRAW FBO + glBlitFramebuffer 从读帧
/// 缓冲拷贝深度。颜色路径透传。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCopyTexImage2D(
    target: u32,
    level: i32,
    internalformat: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    border: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 本调用*定义* level，internalformat 是调用方的选择。但应用重拷贝
        // 已是深度纹理的 level 时需要 blit 路径——查询现有格式保护该场景。
        let mut internalformat = internalformat;
        let mut existing = 0i32;
        (dispatch.get_tex_level_parameter_iv)(
            target,
            level,
            GL_TEXTURE_INTERNAL_FORMAT,
            &mut existing,
        );
        if !is_depth_format(internalformat) && is_depth_format(existing as u32) {
            internalformat = existing as u32;
        }

        if is_depth_format(internalformat) {
            // 深度路径：分配 + 临时 DRAW FBO 挂 DEPTH_ATTACHMENT + blit。
            let mut fmt = GL_DEPTH_COMPONENT;
            let mut typ = GL_UNSIGNED_INT;
            let mut ifmt = internalformat;
            internal_convert(dispatch, &mut ifmt, Some(&mut typ), Some(&mut fmt), false);
            (dispatch.tex_image_2d)(
                target,
                level,
                ifmt as i32,
                width,
                height,
                border,
                fmt,
                typ,
                std::ptr::null(),
            );

            let mut prev_draw_fbo = 0i32;
            (dispatch.get_integerv)(GL_DRAW_FRAMEBUFFER_BINDING, &mut prev_draw_fbo);
            let mut temp_fbo = 0u32;
            (dispatch.gen_framebuffers)(1, &mut temp_fbo);
            (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, temp_fbo);

            let mut current_tex = 0i32;
            (dispatch.get_integerv)(get_binding_for_target(target), &mut current_tex);
            (dispatch.framebuffer_texture_2d)(
                GL_DRAW_FRAMEBUFFER,
                GL_DEPTH_ATTACHMENT,
                target,
                current_tex as u32,
                level,
            );

            if (dispatch.check_framebuffer_status)(GL_DRAW_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
                (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
                (dispatch.delete_framebuffers)(1, &temp_fbo);
                return;
            }

            // blit 前 flush：Adreno 保留未提交的 pending 深度写入，直接 blit
            // 读到的是内存中的旧值（MG 实测注释）。
            (dispatch.flush)();
            (dispatch.blit_framebuffer)(
                x,
                y,
                x + width,
                y + height,
                0,
                0,
                width,
                height,
                GL_DEPTH_BUFFER_BIT,
                GL_NEAREST,
            );

            (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
            (dispatch.delete_framebuffers)(1, &temp_fbo);

            internalformat = ifmt;
        } else {
            (dispatch.copy_tex_image_2d)(
                target,
                level,
                internalformat,
                x,
                y,
                width,
                height,
                border,
            );
        }

        // 影子对象表（MG GET_TEXTURE_OBJECT）。
        if let Some(desktop_id) = bound_texture_for_target(target) {
            meta_get_mut(desktop_id, |meta| {
                meta.target = normalize_target(target);
                meta.internal_format = internalformat;
                meta.width = width;
                meta.height = height;
                meta.depth = 1;
                meta.swizzle = [
                    GL_RED as i32,
                    GL_GREEN as i32,
                    GL_BLUE as i32,
                    GL_ALPHA as i32,
                ];
            });
        }
    });
}

/// glCopyTexSubImage2D — GL 1.1（MG texture.cpp:1250 移植）。
///
/// 深度/深度模板目标在 GLES 无 glCopyTexSubImage2D 语义：用临时 DRAW FBO +
/// glBlitFramebuffer 按 depth/stencil mask 拷贝。颜色路径透传。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCopyTexSubImage2D(
    target: u32,
    level: i32,
    xoffset: i32,
    yoffset: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 目标格式决定深度/颜色路径。影子记录优先（免每帧驱动往返），
        // 无记录时问驱动并写回影子。记录不分 level（同一纹理不会混合
        // 深度与颜色 level）。
        let mut internal_format = 0u32;
        let copy_dst = bound_texture_for_target(target);
        if let Some(id) = copy_dst {
            let meta = meta_get(id);
            if meta.internal_format != 0 {
                internal_format = meta.internal_format;
            }
        }
        if internal_format == 0 {
            let mut q = 0i32;
            (dispatch.get_tex_level_parameter_iv)(
                target,
                level,
                GL_TEXTURE_INTERNAL_FORMAT,
                &mut q,
            );
            internal_format = q as u32;
            if let Some(id) = copy_dst {
                meta_get_mut(id, |meta| meta.internal_format = internal_format);
            }
        }

        let depth_stencil = is_depth_stencil_format(internal_format);
        if depth_stencil || is_depth_format(internal_format) {
            let attachment = if depth_stencil {
                GL_DEPTH_STENCIL_ATTACHMENT
            } else {
                GL_DEPTH_ATTACHMENT
            };
            let mask = if depth_stencil {
                GL_DEPTH_BUFFER_BIT | GL_STENCIL_BUFFER_BIT
            } else {
                GL_DEPTH_BUFFER_BIT
            };

            // 读帧缓冲是源，保持调用方原样；只借用 DRAW 绑定并归还。
            let mut prev_draw_fbo = 0i32;
            (dispatch.get_integerv)(GL_DRAW_FRAMEBUFFER_BINDING, &mut prev_draw_fbo);

            let mut temp_fbo = 0u32;
            (dispatch.gen_framebuffers)(1, &mut temp_fbo);
            (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, temp_fbo);

            let mut current_tex = 0i32;
            (dispatch.get_integerv)(get_binding_for_target(target), &mut current_tex);
            (dispatch.framebuffer_texture_2d)(
                GL_DRAW_FRAMEBUFFER,
                attachment,
                target,
                current_tex as u32,
                level,
            );

            if (dispatch.check_framebuffer_status)(GL_DRAW_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
                if !COPY_DEPTH_FBO_INCOMPLETE_WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!(
                        "[FluorateGL] glCopyTexSubImage2D: 深度目标 (internalformat 0x{:04X}) 无法构成完整帧缓冲，拷贝已跳过 (后续调用将静默)",
                        internal_format
                    );
                }
                (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
                (dispatch.delete_framebuffers)(1, &temp_fbo);
                return;
            }

            // glBlitFramebuffer 对深度/模板要求源与目标格式严格一致；失败是
            // 静默的（本层 glGetError 不报告），先清空队列。
            while (dispatch.get_error)() != 0 {}
            // flush 保真：Adreno 首次深度 blit 前必须 flush（MG 实测）。
            (dispatch.flush)();
            (dispatch.blit_framebuffer)(
                x,
                y,
                x + width,
                y + height,
                xoffset,
                yoffset,
                xoffset + width,
                yoffset + height,
                mask,
                GL_NEAREST,
            );
            if (dispatch.get_error)() != 0 {
                if !COPY_DEPTH_BLIT_REFUSED_WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!(
                        "[FluorateGL] glCopyTexSubImage2D: 驱动拒绝深度 blit 到 internalformat 0x{:04X}——GLES 要求源帧缓冲与目标格式完全一致，未拷贝任何内容 (后续调用将静默)",
                        internal_format
                    );
                }
            }

            (dispatch.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
            (dispatch.delete_framebuffers)(1, &temp_fbo);
        } else {
            (dispatch.copy_tex_sub_image_2d)(target, level, xoffset, yoffset, x, y, width, height);
        }
    });
}

/// glCopyTexSubImage3D — GL 1.2（GLES 3.0 原生透传；MG 同为透传）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCopyTexSubImage3D(
    target: u32,
    level: i32,
    xoffset: i32,
    yoffset: i32,
    zoffset: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.copy_tex_sub_image_3d)(
            target, level, xoffset, yoffset, zoffset, x, y, width, height,
        );
    });
}

// ===========================================================================
// 压缩纹理能力（保留的原实现体系）
// ===========================================================================

/// 判断 internalformat 是否为已知的压缩纹理格式。
/// 用于 `glCompressedTexImage*` 系列函数，防止将非压缩格式透传给 GLES 驱动导致 GL_INVALID_ENUM。
fn is_compressed_format(internalformat: u32) -> bool {
    matches!(
        internalformat,
        // S3TC / DXT
        0x83F0 | 0x83F1 | 0x83F2 | 0x83F3
        // ETC2 / EAC（含有符号变体 0x9279-0x927B，GLES 3.0 core）
        | 0x9274 | 0x9275 | 0x9276 | 0x9277 | 0x9278 | 0x9279 | 0x927A | 0x927B
        // RGTC（BC4/BC5，桌面 GL 3.0+，GLES 需 GL_ARB_texture_compression_rgtc 类扩展）
        | 0x8DBB | 0x8DBC | 0x8DBD | 0x8DBE
        // BPTC（BC6H/BC7，桌面 GL 4.2+，GLES 需 GL_ARB_texture_compression_bptc 类扩展）
        | 0x8E8C | 0x8E8D | 0x8E8E | 0x8E8F
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

/// GLES 驱动支持的压缩格式列表缓存（OnceLock，首次查询后恒定）。
///
/// S3TC 不是 GLES core 压缩格式（强制格式为 ETC2/EAC），部分移动驱动支持
/// GL_EXT_texture_compression_s3tc、多数不支持。透传不支持的格式必然
/// INVALID_ENUM，故上传前按 glGetIntegerv 的格式列表做一次运行时能力判断。
static COMPRESSED_FORMATS_SUPPORTED: OnceLock<Vec<u32>> = OnceLock::new();

/// 查询 GLES 驱动是否支持指定压缩格式。
///
/// 通过 GL_NUM_COMPRESSED_TEXTURE_FORMATS(0x86A2) + GL_COMPRESSED_TEXTURE_FORMATS(0x86A3)
/// 读取驱动支持的压缩格式列表（需 GL 上下文已绑定，由调用方保证在
/// with_gles_dispatch 内执行）。结果缓存在 OnceLock，仅首次真正查询。
fn gles_supports_compressed_format(
    dispatch: &backend::dispatch::GlesDispatch,
    format: u32,
) -> bool {
    let supported = COMPRESSED_FORMATS_SUPPORTED.get_or_init(|| {
        const GL_NUM_COMPRESSED_TEXTURE_FORMATS: u32 = 0x86A2;
        const GL_COMPRESSED_TEXTURE_FORMATS: u32 = 0x86A3;
        let mut count = 0i32;
        unsafe { (dispatch.get_integerv)(GL_NUM_COMPRESSED_TEXTURE_FORMATS, &mut count) };
        if count <= 0 {
            return Vec::new();
        }
        let mut formats = vec![0i32; count as usize];
        unsafe { (dispatch.get_integerv)(GL_COMPRESSED_TEXTURE_FORMATS, formats.as_mut_ptr()) };
        formats.into_iter().map(|f| f as u32).collect()
    });
    supported.contains(&format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_align_pow2() {
        assert_eq!(width_align(3, 4), 4);
        assert_eq!(width_align(4, 4), 4);
        assert_eq!(width_align(5, 8), 8);
        assert_eq!(width_align(10, 1), 10);
        // 非 2 次幂对齐：原样返回
        assert_eq!(width_align(7, 3), 7);
        assert_eq!(width_align(7, 0), 7);
    }

    #[test]
    fn checked_area_overflow() {
        assert_eq!(checked_area(16, 16, 1, 4), Some(1024));
        assert_eq!(checked_area(0, 16, 1, 4), None);
        assert_eq!(checked_area(-1, 16, 1, 4), None);
        assert_eq!(checked_area(16, 16, 1, 0), None);
        // 2^40 上限内（i32 尺寸最大乘积 < 2^63，此处验证大尺寸不 panic）
        assert!(checked_area(i32::MAX, 1, 1, 4).is_some());
        assert!(checked_area(i32::MAX, i32::MAX, 1, 4).is_none());
    }

    #[test]
    fn normalize_target_folds_cube_faces() {
        assert_eq!(normalize_target(GL_TEXTURE_2D), GL_TEXTURE_2D);
        assert_eq!(
            normalize_target(GL_TEXTURE_CUBE_MAP_POSITIVE_X),
            GL_TEXTURE_CUBE_MAP
        );
        assert_eq!(
            normalize_target(0x8517 /* POSITIVE_Y */),
            GL_TEXTURE_CUBE_MAP
        );
        assert_eq!(
            normalize_target(GL_TEXTURE_CUBE_MAP_NEGATIVE_Z),
            GL_TEXTURE_CUBE_MAP
        );
        assert_eq!(normalize_target(GL_TEXTURE_3D), GL_TEXTURE_3D);
    }

    #[test]
    fn depth_format_classification() {
        assert!(is_depth_format(GL_DEPTH_COMPONENT));
        assert!(is_depth_format(GL_DEPTH_COMPONENT24));
        assert!(is_depth_format(GL_DEPTH_COMPONENT32));
        assert!(!is_depth_format(GL_RGBA8));
        assert!(!is_depth_format(GL_DEPTH24_STENCIL8));
        assert!(is_depth_stencil_format(GL_DEPTH_STENCIL));
        assert!(is_depth_stencil_format(GL_DEPTH24_STENCIL8));
        assert!(is_depth_stencil_format(GL_DEPTH32F_STENCIL8));
        assert!(!is_depth_stencil_format(GL_DEPTH_COMPONENT24));
    }

    #[test]
    fn normalize_internal_format_mg_alignment() {
        // MG 语义：DEPTH_COMPONENT32 → 24（无 OES_depth32，24 保持 unorm 且可 blit）
        assert_eq!(
            normalize_internal_format(GL_DEPTH_COMPONENT32),
            GL_DEPTH_COMPONENT24
        );
        // unsized → sized 兜底保留
        assert_eq!(normalize_internal_format(GL_RGBA), GL_RGBA8);
        assert_eq!(normalize_internal_format(GL_RGB), GL_RGB8);
        assert_eq!(
            normalize_internal_format(GL_DEPTH_STENCIL),
            GL_DEPTH24_STENCIL8
        );
        // sized 原样透传
        assert_eq!(normalize_internal_format(GL_RGBA8), GL_RGBA8);
    }

    #[test]
    fn nlevel_shrinks_to_one() {
        assert_eq!(nlevel(64, 3), 8);
        assert_eq!(nlevel(1, 1), 1);
        assert_eq!(nlevel(0, 0), 0);
        assert_eq!(nlevel(16, 5), 1);
    }

    #[test]
    fn upload_rules_lookup() {
        // 精确命中（非 adapt-only）
        let r = find_upload_rule(GL_BGRA, GL_UNSIGNED_BYTE, 0).expect("BGRA rule");
        assert_eq!(r.src_size, 4);
        assert_eq!(r.channels, 4);
        // adapt-only：目标通道数与源不同才启用
        assert!(find_upload_rule(GL_RGB, GL_UNSIGNED_BYTE, 0).is_none());
        let r = find_upload_rule(GL_RGB, GL_UNSIGNED_BYTE, GL_RGBA).expect("RGB→RGBA adapt");
        assert_eq!(r.channels, 3);
        // 未知组合
        assert!(find_upload_rule(GL_RGBA, GL_FLOAT, 0).is_none());
    }

    #[test]
    fn channels_of_basic() {
        assert_eq!(channels_of(GL_RGBA), 4);
        assert_eq!(channels_of(GL_BGRA), 4);
        assert_eq!(channels_of(GL_RGB), 3);
        assert_eq!(channels_of(GL_BGR), 3);
        assert_eq!(channels_of(GL_RED), 0);
    }

    /// 在栈上做解码往返：解码器输出 RGBA 字节序，编码器读回。
    #[test]
    fn bgra_decode_encode_roundtrip() {
        let src = [10u8, 20, 30, 40]; // B,G,R,A
        let mut px = [0u8; 4];
        unsafe { dec_bgra_u8(src.as_ptr(), px.as_mut_ptr()) };
        assert_eq!(px, [30, 20, 10, 40]); // R,G,B,A
        let mut out = [0u8; 4];
        unsafe { enc_bgra_u8(px.as_ptr(), out.as_mut_ptr()) };
        assert_eq!(out, src);
    }

    #[test]
    fn packed_8888_rev_roundtrip() {
        // RGBA 打包 8888_REV：R=7..0 G=15..8 B=23..16 A=31..24
        let src = [0x12u8, 0x34, 0x56, 0x78];
        let mut px = [0u8; 4];
        unsafe { dec_rgba_8888_rev(src.as_ptr(), px.as_mut_ptr()) };
        assert_eq!(px, [0x12, 0x34, 0x56, 0x78]);
        let mut out = [0u8; 4];
        unsafe { enc_rgba_8888_rev(px.as_ptr(), out.as_mut_ptr()) };
        assert_eq!(out, src);
    }

    #[test]
    fn bgra_1555_rev_roundtrip() {
        // B=4..0 G=9..5 R=14..10 A=15，内存序低字节在前
        let src = [0x1fu8, 0x00]; // 0x001f：R=0 G=0 B=31 A=0
        let mut px = [0u8; 4];
        unsafe { dec_bgra_1555_rev(src.as_ptr(), px.as_mut_ptr()) };
        assert_eq!(px, [0, 0, 255, 0]);
        // 编码往返：31 → 255 → 31
        let mut out = [0u8; 2];
        unsafe { enc_bgra_1555_rev(px.as_ptr(), out.as_mut_ptr()) };
        assert_eq!(out, src);
        // A=1 时 alpha 255
        let src2 = [0x1fu8, 0x80];
        let mut px2 = [0u8; 4];
        unsafe { dec_bgra_1555_rev(src2.as_ptr(), px2.as_mut_ptr()) };
        assert_eq!(px2[3], 255);
    }

    #[test]
    fn bgra_4444_rev_roundtrip() {
        // B=3..0 G=7..4 R=11..8 A=15..12
        let src = [0x0fu8, 0x00]; // R=0 G=0 B=15 A=0
        let mut px = [0u8; 4];
        unsafe { dec_bgra_4444_rev(src.as_ptr(), px.as_mut_ptr()) };
        assert_eq!(px, [0, 0, 255, 0]);
        let mut out = [0u8; 2];
        unsafe { enc_bgra_4444_rev(px.as_ptr(), out.as_mut_ptr()) };
        assert_eq!(out, src);
    }

    #[test]
    fn readback_rules_cover_mc_pairs() {
        // MC 常用读回组合必须可重编码
        assert!(
            READBACK_RULES
                .iter()
                .any(|r| r.format == GL_BGRA && r.type_ == GL_UNSIGNED_BYTE)
        );
        assert!(
            READBACK_RULES
                .iter()
                .any(|r| r.format == GL_RGB && r.type_ == GL_UNSIGNED_BYTE)
        );
        assert!(
            READBACK_RULES
                .iter()
                .any(|r| r.format == GL_RGBA && r.type_ == GL_UNSIGNED_INT_8_8_8_8_REV)
        );
    }

    #[test]
    fn decode_clear_value_unsigned_byte() {
        let data = [10u8, 20, 30, 40];
        let mut rgba = [0f32; 4];
        let mut depth = 0f32;
        let mut stencil = 0i32;
        assert!(decode_clear_value(
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            data.as_ptr() as *const c_void,
            &mut rgba,
            &mut depth,
            &mut stencil
        ));
        assert!((rgba[0] - 10.0 / 255.0).abs() < 1e-6);
        assert!((rgba[3] - 40.0 / 255.0).abs() < 1e-6);
        // BGR 红蓝交换
        assert!(decode_clear_value(
            GL_BGR,
            GL_UNSIGNED_BYTE,
            data.as_ptr() as *const c_void,
            &mut rgba,
            &mut depth,
            &mut stencil
        ));
        assert!((rgba[0] - 30.0 / 255.0).abs() < 1e-6);
        assert!((rgba[2] - 10.0 / 255.0).abs() < 1e-6);
        // 浮点 RGBA 也支持（MG decode_clear_value 语义）
        let fdata = [0.5f32, 0.25, 0.75, 1.0];
        assert!(decode_clear_value(
            GL_RGBA,
            GL_FLOAT,
            fdata.as_ptr() as *const c_void,
            &mut rgba,
            &mut depth,
            &mut stencil
        ));
        assert!((rgba[0] - 0.5).abs() < 1e-6);
        // 未知组合拒绝（类型不是 UBYTE/FLOAT 时）
        assert!(!decode_clear_value(
            GL_RGBA,
            GL_UNSIGNED_SHORT,
            data.as_ptr() as *const c_void,
            &mut rgba,
            &mut depth,
            &mut stencil
        ));
    }

    #[test]
    fn map_tex_target_folds_legacy_targets() {
        // 1D/3D/RECTANGLE → 2D；PROXY 变体 → PROXY_TEXTURE_2D（MG map_tex_target）
        assert_eq!(map_tex_target(GL_TEXTURE_1D), GL_TEXTURE_2D);
        assert_eq!(map_tex_target(GL_TEXTURE_3D), GL_TEXTURE_2D);
        assert_eq!(map_tex_target(GL_TEXTURE_RECTANGLE), GL_TEXTURE_2D);
        assert_eq!(map_tex_target(GL_PROXY_TEXTURE_1D), GL_PROXY_TEXTURE_2D);
        assert_eq!(map_tex_target(GL_PROXY_TEXTURE_3D), GL_PROXY_TEXTURE_2D);
        assert_eq!(
            map_tex_target(GL_PROXY_TEXTURE_RECTANGLE),
            GL_PROXY_TEXTURE_2D
        );
        // 原生目标原样
        assert_eq!(map_tex_target(GL_TEXTURE_2D), GL_TEXTURE_2D);
        assert_eq!(map_tex_target(GL_TEXTURE_CUBE_MAP), GL_TEXTURE_CUBE_MAP);
    }

    #[test]
    fn proxy_nlevel_answer() {
        // glGetTexLevelParameteriv 的 PROXY 回答：nlevel(影子宽, level)
        assert_eq!(nlevel(64, 0), 64);
        assert_eq!(nlevel(64, 3), 8);
        assert_eq!(nlevel(0, 0), 0);
    }

    #[test]
    fn texture_buffer_emulation_switch() {
        set_texture_buffer_emulation(false);
        assert!(!texture_buffer_emulation_enabled());
        set_texture_buffer_emulation(true);
        assert!(texture_buffer_emulation_enabled());
        set_texture_buffer_emulation(false);
    }
}
