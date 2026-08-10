use crate::backend;
use crate::state;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

// glMapBufferRange access bits（桌面 GL 与 GLES 共享的低 16 位语义）
// GLES 3.1 仅支持 READ/WRITE/INVALIDATE_RANGE/INVALIDATE_BUFFER/FLUSH_EXPLICIT/UNSYNCHRONIZED，
// 不支持 PERSISTENT(0x0040)/COHERENT(0x0080)（桌面 GL 4.4 / GL_ARB_buffer_storage 引入；
// GLES 需 GL_EXT_buffer_storage）。
const GL_MAP_READ_BIT: u32 = 0x0001;
const GL_MAP_WRITE_BIT: u32 = 0x0002;
const GL_MAP_FLUSH_EXPLICIT_BIT: u32 = 0x0010;
const GL_MAP_PERSISTENT_BIT: u32 = 0x0040;
const GL_MAP_COHERENT_BIT: u32 = 0x0080;
/// GL_DYNAMIC_STORAGE_BIT（glBufferStorage flags，GL 4.4 / GL_ARB_buffer_storage）
const GL_DYNAMIC_STORAGE_BIT: u32 = 0x0100;

/// GL_PARAMETER_BUFFER（GL 4.6 引入，glMultiDraw*IndirectCount 的 count 来源）
/// GLES 不识别该 target，下传会触发 GL_INVALID_ENUM，仅在 state 中记录绑定。
const GL_PARAMETER_BUFFER: u32 = 0x80EE;
/// GL_COPY_WRITE_BUFFER：MG（buffer.cpp:991-1012 borrowed_target_t）借用该通用
/// target 在单次调用期间代持 GL_PARAMETER_BUFFER 的绑定（绑定查询枚举同值 0x8F37）
const GL_COPY_WRITE_BUFFER: u32 = 0x8F37;
/// GL_COPY_READ_BUFFER：GetBufferSubData 的 COPY 中转（源 buffer 不可 map 时借用）
const GL_COPY_READ_BUFFER: u32 = 0x8F36;
/// GL_ELEMENT_ARRAY_BUFFER：IBO 绑定是 VAO state（per-VAO 记录用）
const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
/// P2：atomic counter buffer → SSBO 绑定转发常量（与 drawing.rs 定义保持一致）
const GL_ATOMIC_COUNTER_BUFFER: u32 = 0x92C0;
const GL_SHADER_STORAGE_BUFFER: u32 = 0x90F2;
/// 错误码（set_gl_error 上报用；exports.rs 内以字面量使用，此处集中命名）
const GL_INVALID_ENUM: u32 = 0x0500;
const GL_INVALID_VALUE: u32 = 0x0501;

// ===== glTexBuffer 模拟（MG buffer.cpp:793-960）使用的常量 =====
const GL_TEXTURE_BUFFER: u32 = 0x8C2A;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_TEXTURE0: u32 = 0x84C0;
const GL_PIXEL_UNPACK_BUFFER: u32 = 0x88EC;
const GL_PIXEL_UNPACK_BUFFER_BINDING: u32 = 0x88EF;
const GL_BUFFER_SIZE: u32 = 0x8764;
const GL_ACTIVE_TEXTURE: u32 = 0x84E0;
const GL_TEXTURE_BINDING_2D: u32 = 0x8069;
const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;
const GL_UNPACK_ROW_LENGTH: u32 = 0x0CF2;
const GL_UNPACK_SKIP_PIXELS: u32 = 0x0CF4;
const GL_UNPACK_SKIP_ROWS: u32 = 0x0CF3;
const GL_NEAREST: u32 = 0x2600;
const GL_CLAMP_TO_EDGE: u32 = 0x812F;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_TEXTURE_BASE_LEVEL: u32 = 0x813C;
const GL_TEXTURE_MAX_LEVEL: u32 = 0x813D;
// transfer 对（get_internal_format_transfer 输出）
const GL_RED: u32 = 0x1903;
const GL_RED_INTEGER: u32 = 0x8D94;
const GL_RG: u32 = 0x8227;
const GL_RG_INTEGER: u32 = 0x8228;
const GL_RGB: u32 = 0x1907;
const GL_RGB_INTEGER: u32 = 0x8D98;
const GL_RGBA: u32 = 0x1908;
const GL_RGBA_INTEGER: u32 = 0x8D99;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_BYTE: u32 = 0x1400;
const GL_SHORT: u32 = 0x1402;
const GL_UNSIGNED_SHORT: u32 = 0x1403;
const GL_INT: u32 = 0x1404;
const GL_UNSIGNED_INT: u32 = 0x1405;
const GL_FLOAT: u32 = 0x1406;
const GL_HALF_FLOAT: u32 = 0x140B;
// 内部格式（sized internalformat）
const GL_R8: u32 = 0x8229;
const GL_R16: u32 = 0x822A;
const GL_R8I: u32 = 0x8231;
const GL_R8UI: u32 = 0x8232;
const GL_R16I: u32 = 0x8233;
const GL_R16UI: u32 = 0x8234;
const GL_R16F: u32 = 0x822D;
const GL_R32I: u32 = 0x8235;
const GL_R32UI: u32 = 0x8236;
const GL_R32F: u32 = 0x822E;
const GL_RG8: u32 = 0x822B;
const GL_RG16: u32 = 0x822C;
const GL_RG8I: u32 = 0x8237;
const GL_RG8UI: u32 = 0x8238;
const GL_RG16I: u32 = 0x8239;
const GL_RG16UI: u32 = 0x823A;
const GL_RG16F: u32 = 0x822F;
const GL_RG32I: u32 = 0x823B;
const GL_RG32UI: u32 = 0x823C;
const GL_RG32F: u32 = 0x8230;
const GL_RGB8: u32 = 0x8051;
const GL_RGB16: u32 = 0x8054;
const GL_RGB8I: u32 = 0x8D8F;
const GL_RGB8UI: u32 = 0x8D7D;
const GL_RGB16I: u32 = 0x8D89;
const GL_RGB16UI: u32 = 0x8D77;
const GL_RGB16F: u32 = 0x881B;
const GL_RGB32I: u32 = 0x8D83;
const GL_RGB32UI: u32 = 0x8D71;
const GL_RGB32F: u32 = 0x8815;
const GL_RGBA8: u32 = 0x8058;
const GL_RGBA16: u32 = 0x805B;
const GL_RGBA8I: u32 = 0x8D8E;
const GL_RGBA8UI: u32 = 0x8D7C;
const GL_RGBA16I: u32 = 0x8D88;
const GL_RGBA16UI: u32 = 0x8D76;
const GL_RGBA16F: u32 = 0x881A;
const GL_RGBA32I: u32 = 0x8D82;
const GL_RGBA32UI: u32 = 0x8D70;
const GL_RGBA32F: u32 = 0x8814;
const GL_DEPTH_COMPONENT16: u32 = 0x81A5;
const GL_DEPTH_COMPONENT24: u32 = 0x81A6;
const GL_DEPTH_COMPONENT32: u32 = 0x81A7;
const GL_DEPTH_COMPONENT32F: u32 = 0x8CAC;
const GL_DEPTH24_STENCIL8: u32 = 0x88F0;
const GL_DEPTH32F_STENCIL8: u32 = 0x8CAD;
const GL_STENCIL_INDEX8: u32 = 0x8D48;
const GL_COMPRESSED_RGB_S3TC_DXT1_EXT: u32 = 0x83F0;
const GL_COMPRESSED_RGBA_S3TC_DXT1_EXT: u32 = 0x83F1;
const GL_COMPRESSED_RGBA_S3TC_DXT3_EXT: u32 = 0x83F2;
const GL_COMPRESSED_RGBA_S3TC_DXT5_EXT: u32 = 0x83F3;

/// buffer_coherent_as_flush 配置（MG config/settings.cpp:186 语义）：
/// ANGLE 后端 → false（ANGLE 自管同步），其余后端（system/llvmpipe）→ true。
/// 影响三处行为（与 MG buffer.cpp 完全对齐）：
/// - glMapBufferRange：清除 GL_MAP_FLUSH_EXPLICIT_BIT（映射即自动可见）
/// - glBufferStorage：PERSISTENT/DYNAMIC 时追加 WRITE|COHERENT|PERSISTENT
/// - glFlushMappedBufferRange：no-op
/// 惰性初始化（OnceLock），首次调用时从环境配置推断一次。
static BUFFER_COHERENT_AS_FLUSH: OnceLock<bool> = OnceLock::new();
fn buffer_coherent_as_flush() -> bool {
    *BUFFER_COHERENT_AS_FLUSH.get_or_init(|| {
        let cfg = crate::config::Config::from_env();
        cfg.backend != crate::config::Backend::Angle
    })
}

/// 将桌面 GL 的 glMapBuffer access 枚举/位值翻译为 GLES 3.1 支持的位。
///
/// 仅服务 glMapBuffer 的兜底分支（MG buffer.cpp:1055-1066 对未知 access 直接返回
/// nullptr，我们保留更宽容的位剥离：fail-open，避免未知枚举静默失败）：
/// - PERSISTENT/COHERENT 是 GLES 3.1 不支持位，剥离（GLES 3.1 下透传会失败）
/// - 剥离后若无任何有效读写位，补 GL_MAP_WRITE_BIT 避免 GLES 返回 NULL
///
/// 注意：glMapBufferRange 不再走此翻译——MG 语义为直接透传（驱动若支持
/// GL_EXT_buffer_storage 则 PERSISTENT/COHERENT 位合法），见 glMapBufferRange。
fn translate_map_access(access: u32) -> u32 {
    let mut out = access & !GL_MAP_PERSISTENT_BIT;
    out &= !GL_MAP_COHERENT_BIT;
    if out & (GL_MAP_READ_BIT | GL_MAP_WRITE_BIT) == 0 {
        out |= GL_MAP_WRITE_BIT;
    }
    out
}

/// buffer stub 降级相关首次告警标志（避免每帧刷屏）
/// glMapBuffer：GLES 无此函数，用 glMapBufferRange 模拟
static MAP_BUFFER_WARNED: AtomicBool = AtomicBool::new(false);
/// glGetBufferSubData：GLES 无此函数，用 glMapBufferRange 模拟
static GET_BUFFER_SUB_DATA_WARNED: AtomicBool = AtomicBool::new(false);
/// glTexBuffer/glTexBufferRange：GLES 3.2 core，项目 3.1 前提下可能 stub
static TEX_BUFFER_STUB_WARNED: AtomicBool = AtomicBool::new(false);

/// buffer desktop ID 查找失败首次告警标志
/// 触发场景：跨线程绑定或资源已释放
static BUFFER_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);
/// glTexBuffer/glTexBufferRange 的 buffer ID 查找失败首次告警标志
static TEX_BUFFER_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glMapBuffer 不可用，降级为 glMapBufferRange。
fn warn_map_buffer_unavailable() {
    if !MAP_BUFFER_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glMapBuffer: glMapBufferRange not available, returning null (后续调用将静默返回 null)"
        );
    }
}

/// 首次告警：glGetBufferSubData 不可用，降级为 glMapBufferRange。
fn warn_get_buffer_sub_data_unavailable() {
    if !GET_BUFFER_SUB_DATA_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glGetBufferSubData: both sub_data and map_range unavailable (后续调用将静默跳过)"
        );
    }
}

/// 首次告警：glTexBuffer/glTexBufferRange 为 stub，已忽略。
fn warn_tex_buffer_stub(fname: &str) {
    if !TEX_BUFFER_STUB_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 不支持 GL_EXT_texture_buffer，已忽略 (后续调用将静默跳过)",
            fname
        );
    }
}

/// 首次告警：buffer desktop ID 未在 IdMap 中找到。
fn warn_buffer_id_miss(fname: &str, target: u32, desktop_id: u32) {
    if !BUFFER_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: target 0x{:04X} desktop ID {} not found in IdMap, unbinding (跨线程或资源已释放，后续将静默降级)",
            fname,
            target,
            desktop_id
        );
    }
}

/// 首次告警：glTexBuffer/glTexBufferRange 的 buffer desktop ID 未在 IdMap 中找到。
fn warn_tex_buffer_id_miss(fname: &str, target: u32, desktop_id: u32) {
    if !TEX_BUFFER_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: target 0x{:04X} desktop ID {} not found in IdMap, unbinding (跨线程或资源已释放，后续将静默降级)",
            fname,
            target,
            desktop_id
        );
    }
}

/// 记录 glBufferData/glBufferStorage 的 size（MG buffer.cpp:1020 set_buffer_data_size
/// 等价物），供 getter 查询 GL_BUFFER_SIZE 等使用（跨域协调点：getter.rs 域对接）。
fn record_buffer_size(target: u32, size: isize) {
    let desktop_id = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    if let Some(desktop_id) = desktop_id {
        let recorded = if size > 0 { size as usize } else { 0 };
        state::with_state(|s| {
            s.buffer_sizes.insert(desktop_id, recorded);
        });
    }
}

/// 懒创建：确保桌面 buffer 在 GLES 后端存在（MG find_real_buffer + 懒 gen 等价物）。
///
/// - `desktop_id == 0` → 返回 0（解绑语义）
/// - 未登记（宿主从未 glGenBuffers）→ 返回 0（调用方走 warn+绑 0 路径，保持既有语义）
/// - 已登记未创建（alloc_pending）→ 首次真实使用：glGenBuffers 一次并登记映射
///   （MG lazy 状态机核心：MC 大量 gen 不用的 buffer 名永不触碰驱动，修复
///   Adreno 高区块数崩溃）
/// - 已创建 → 直接返回映射
fn ensure_gles_buffer(desktop_id: u32) -> u32 {
    if desktop_id == 0 {
        return 0;
    }
    if !state::with_state_ref(|s| s.buffers.contains(desktop_id)) {
        return 0;
    }
    if let Some(gles_id) = state::with_state_ref(|s| s.buffers.get_gles(desktop_id)) {
        return gles_id;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let mut gles_id = 0u32;
        (dispatch.gen_buffers)(1, &mut gles_id);
        state::with_state(|s| s.buffers.bind_gles(desktop_id, gles_id));
        log::debug!(
            "[FluorateGL] lazy buffer create: desktop {} -> GLES {} (首次真实使用)",
            desktop_id,
            gles_id
        );
        gles_id
    })
}

/// glTexBuffer 模拟的首次告警标志（对应 MG BU_WARN_ONCE 语义，避免刷屏）
/// 拒绝原因：内部格式无 texel 大小表条目
static TEX_BUFFER_EMULATE_SIZE_WARNED: AtomicBool = AtomicBool::new(false);
/// 拒绝原因：内部格式无 GLES 可接受的传输对
static TEX_BUFFER_EMULATE_TRANSFER_WARNED: AtomicBool = AtomicBool::new(false);
/// 拒绝原因：buffer 太小容不下一个 texel
static TEX_BUFFER_EMULATE_SMALL_WARNED: AtomicBool = AtomicBool::new(false);

fn warn_tex_buffer_emulate_once(flag: &AtomicBool, msg: &str) {
    if !flag.swap(true, Ordering::Relaxed) {
        log::warn!("[FluorateGL] {}", msg);
    }
}

/// texture buffer 模拟开关（MG gles/loader.cpp:160-162 set_hardware）：
/// GLES <= 3.1 → 模拟（GLES 3.1 无 GL_EXT_texture_buffer 或驱动差异大）；
/// GLES >= 3.2 → 原生 glTexBuffer 路径。
/// 查询 capabilities（OnceLock 缓存，廉价）。
fn emulate_texture_buffer() -> bool {
    !crate::backend::capabilities().version.at_least(3, 2)
}

/// 内部格式 → GLES 可接受的 (format, type) 传输对（MG buffer.cpp:634-680
/// get_internal_format_transfer）。ES 校验 internalformat/format/type 三元组，
/// 硬编码错误对会让 glTexImage2D 报 GL_INVALID_OPERATION 且 level 0 未定义。
/// 无合法对的格式（normalised 16-bit 需 EXT_texture_norm16、depth 格式非
/// texture buffer 格式）返回 None，调用方丢弃该调用而不是瞎猜。
fn get_internal_format_transfer(internalformat: u32) -> Option<(u32, u32)> {
    Some(match internalformat {
        GL_R8 => (GL_RED, GL_UNSIGNED_BYTE),
        GL_R8I => (GL_RED_INTEGER, GL_BYTE),
        GL_R8UI => (GL_RED_INTEGER, GL_UNSIGNED_BYTE),
        GL_R16I => (GL_RED_INTEGER, GL_SHORT),
        GL_R16UI => (GL_RED_INTEGER, GL_UNSIGNED_SHORT),
        GL_R16F => (GL_RED, GL_HALF_FLOAT),
        GL_R32I => (GL_RED_INTEGER, GL_INT),
        GL_R32UI => (GL_RED_INTEGER, GL_UNSIGNED_INT),
        GL_R32F => (GL_RED, GL_FLOAT),

        GL_RG8 => (GL_RG, GL_UNSIGNED_BYTE),
        GL_RG8I => (GL_RG_INTEGER, GL_BYTE),
        GL_RG8UI => (GL_RG_INTEGER, GL_UNSIGNED_BYTE),
        GL_RG16I => (GL_RG_INTEGER, GL_SHORT),
        GL_RG16UI => (GL_RG_INTEGER, GL_UNSIGNED_SHORT),
        GL_RG16F => (GL_RG, GL_HALF_FLOAT),
        GL_RG32I => (GL_RG_INTEGER, GL_INT),
        GL_RG32UI => (GL_RG_INTEGER, GL_UNSIGNED_INT),
        GL_RG32F => (GL_RG, GL_FLOAT),

        GL_RGB8 => (GL_RGB, GL_UNSIGNED_BYTE),
        GL_RGB8I => (GL_RGB_INTEGER, GL_BYTE),
        GL_RGB8UI => (GL_RGB_INTEGER, GL_UNSIGNED_BYTE),
        GL_RGB16I => (GL_RGB_INTEGER, GL_SHORT),
        GL_RGB16UI => (GL_RGB_INTEGER, GL_UNSIGNED_SHORT),
        GL_RGB16F => (GL_RGB, GL_HALF_FLOAT),
        GL_RGB32I => (GL_RGB_INTEGER, GL_INT),
        GL_RGB32UI => (GL_RGB_INTEGER, GL_UNSIGNED_INT),
        GL_RGB32F => (GL_RGB, GL_FLOAT),

        GL_RGBA8 => (GL_RGBA, GL_UNSIGNED_BYTE),
        GL_RGBA8I => (GL_RGBA_INTEGER, GL_BYTE),
        GL_RGBA8UI => (GL_RGBA_INTEGER, GL_UNSIGNED_BYTE),
        GL_RGBA16I => (GL_RGBA_INTEGER, GL_SHORT),
        GL_RGBA16UI => (GL_RGBA_INTEGER, GL_UNSIGNED_SHORT),
        GL_RGBA16F => (GL_RGBA, GL_HALF_FLOAT),
        GL_RGBA32I => (GL_RGBA_INTEGER, GL_INT),
        GL_RGBA32UI => (GL_RGBA_INTEGER, GL_UNSIGNED_INT),
        GL_RGBA32F => (GL_RGBA, GL_FLOAT),
        _ => return None,
    })
}

/// 内部格式的 texel 大小（字节）（MG buffer.cpp:682-775 get_internal_format_size）。
/// 与传输表共用同一组常量，两表不会漂移。未知格式返回 0（调用方拒绝）。
fn get_internal_format_size(internalformat: u32) -> usize {
    match internalformat {
        GL_R8 => 1,
        GL_R8I | GL_R8UI => 1,
        GL_R16 => 2,
        GL_R16I | GL_R16UI | GL_R16F => 2,
        GL_R32I | GL_R32UI | GL_R32F => 4,

        GL_RG8 => 2,
        GL_RG8I | GL_RG8UI => 2,
        GL_RG16 => 4,
        GL_RG16I | GL_RG16UI | GL_RG16F => 4,
        GL_RG32I | GL_RG32UI | GL_RG32F => 8,

        GL_RGB8 => 3,
        GL_RGB8I | GL_RGB8UI => 3,
        GL_RGB16 => 6,
        GL_RGB16I | GL_RGB16UI | GL_RGB16F => 6,
        GL_RGB32I | GL_RGB32UI | GL_RGB32F => 12,

        GL_RGBA8 => 4,
        GL_RGBA8I | GL_RGBA8UI => 4,
        GL_RGBA16 => 8,
        GL_RGBA16I | GL_RGBA16UI | GL_RGBA16F => 8,
        GL_RGBA32I | GL_RGBA32UI | GL_RGBA32F => 16,

        GL_DEPTH_COMPONENT16 => 2,
        GL_DEPTH_COMPONENT24 => 3,
        GL_DEPTH_COMPONENT32 => 4,
        GL_DEPTH_COMPONENT32F => 4,
        GL_DEPTH24_STENCIL8 => 4,
        GL_DEPTH32F_STENCIL8 => 5,

        GL_STENCIL_INDEX8 => 1,

        GL_COMPRESSED_RGB_S3TC_DXT1_EXT | GL_COMPRESSED_RGBA_S3TC_DXT1_EXT => 8,
        GL_COMPRESSED_RGBA_S3TC_DXT3_EXT | GL_COMPRESSED_RGBA_S3TC_DXT5_EXT => 16,

        _ => 0,
    }
}

/// MG borrowed_target_t（buffer.cpp:991-1012）等价物。
///
/// GLES 没有 GL_PARAMETER_BUFFER 绑定槽：`requested == GL_PARAMETER_BUFFER` 时，
/// 把参数 buffer 的 GLES id 临时绑到 GL_COPY_WRITE_BUFFER（GLES 定义为无自身语义的
/// 通用 target，借用不可见），调用 `f(dispatch, GL_COPY_WRITE_BUFFER)` 后恢复原绑定；
/// 其他 target 直接 `f(dispatch, requested)`。
///
/// 保存/恢复用驱动查询（GL_COPY_WRITE_BUFFER_BINDING），与 MG 一致——不依赖本层
/// 跟踪状态，能反映所有直连 GLES 的路径造成的真实绑定。
fn with_borrowed_target<R>(
    requested: u32,
    f: impl FnOnce(&backend::dispatch::GlesDispatch, u32) -> R,
) -> R {
    if requested != GL_PARAMETER_BUFFER {
        return backend::with_gles_dispatch(|d| f(d, requested));
    }
    // GL_PARAMETER_BUFFER 绑定的 desktop buffer → 其 GLES id（ensure 懒创建：
    // bind 分支已创建，这里幂等覆盖"未 bind 直接 bufferData"的宿主行为；
    // 未登记 id 返回 0，后续调用自然失败，fail-safe）
    let desktop_id =
        state::with_state_ref(|s| s.bound_buffers_by_target.get(&GL_PARAMETER_BUFFER).copied())
            .unwrap_or(0);
    let gles_id = if desktop_id == 0 {
        0
    } else {
        ensure_gles_buffer(desktop_id)
    };
    backend::with_gles_dispatch(|dispatch| unsafe {
        let mut saved: i32 = 0;
        (dispatch.get_integerv)(GL_COPY_WRITE_BUFFER, &mut saved);
        (dispatch.bind_buffer)(GL_COPY_WRITE_BUFFER, gles_id);
        let r = f(dispatch, GL_COPY_WRITE_BUFFER);
        (dispatch.bind_buffer)(GL_COPY_WRITE_BUFFER, saved as u32);
        r
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenBuffers(n: i32, buffers: *mut u32) {
    // MG 语义（buffer.cpp:470-476 gen_buffer）：只登记 fake id，不创建 GLES 对象。
    // 首次真实使用（bind/upload/map/delete 路径）经 ensure_gles_buffer 懒创建——
    // MC 会创建几万个 buffer name 但大多不使用，eager 创建会压垮 Adreno 驱动
    // （开发者原话：高区块数崩溃）。
    for i in 0..n as isize {
        let desktop_id = state::with_state(|s| s.buffers.alloc_pending());
        log::debug!(
            "[FluorateGL] glGenBuffers: desktop {} (lazy, 未创建 GLES 对象, tid={})",
            desktop_id,
            state::thread_id_u64()
        );
        unsafe {
            *buffers.offset(i) = desktop_id;
        }
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteBuffers(n: i32, buffers: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let desktop_id = *buffers.offset(i);
            // MG（buffer.cpp:487-489）：删除 PARAMETER_BUFFER 绑定的 buffer 时清空
            // 绑定槽——该槽是 multi_draw 读取 count 的唯一数据源，无驱动侧绑定可
            // 交叉校验；且删除的 id 可能被复用，stale 槽会静默指向别人的 buffer。
            if desktop_id != 0 {
                state::with_state(|s| {
                    if s.bound_buffers_by_target.get(&GL_PARAMETER_BUFFER) == Some(&desktop_id) {
                        s.bound_buffers_by_target.remove(&GL_PARAMETER_BUFFER);
                    }
                    s.buffer_sizes.remove(&desktop_id);
                });
            }
            // lazy：仅已创建后端对象的 buffer 需要调用 glDeleteBuffers；
            // 永不使用的 pending buffer 只清记录（MG remove_buffer 语义）
            if let Some(gles_id) = state::with_state_ref(|s| s.buffers.get_gles(desktop_id)) {
                log::debug!(
                    "[FluorateGL] glDeleteBuffers: desktop {} -> GLES {} (deleted, tid={})",
                    desktop_id,
                    gles_id,
                    state::thread_id_u64()
                );
                (dispatch.delete_buffers)(1, &gles_id);
            } else {
                log::debug!(
                    "[FluorateGL] glDeleteBuffers: desktop {} 未创建后端对象（lazy），仅清理记录",
                    desktop_id
                );
            }
            state::with_state(|s| {
                s.buffers.delete(desktop_id);
            });
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindBuffer(target: u32, buffer: u32) {
    // GL_PARAMETER_BUFFER 是 GL 4.6 引入的 target（glMultiDraw*IndirectCount 的 count 来源），
    // GLES 不识别该 target，下传会触发 GL_INVALID_ENUM。仅记录 state 用于 CPU 端模拟
    // （MG buffer.cpp:510-521：绑定由本层跟踪，backing 对象必须存在——否则
    // glBufferData 借用 GL_COPY_WRITE_BUFFER 时没有真实对象可操作）。
    if target == GL_PARAMETER_BUFFER {
        state::with_state(|s| {
            s.bound_buffers_by_target.insert(target, buffer);
        });
        // MG（buffer.cpp:515-519）：backing 对象立即懒创建（已登记的 buffer）；
        // 未登记 id 不创建（read_parameter_buffer_u32 时自然失败，fail-safe）
        if buffer != 0 {
            ensure_gles_buffer(buffer);
        }
        log::debug!(
            "[FluorateGL] glBindBuffer(GL_PARAMETER_BUFFER): desktop {} recorded (not forwarded, tid={})",
            buffer,
            state::thread_id_u64()
        );
        return;
    }

    backend::with_gles_dispatch(|dispatch| unsafe {
        // lazy 状态机：已登记（gen 过）的 buffer 在首次 bind 时创建 GLES 对象；
        // 未登记的 id 保持 warn + 绑 0（既有语义，兼容测试/差分）
        let gles_id = if buffer == 0 {
            0
        } else if !state::with_state_ref(|s| s.buffers.contains(buffer)) {
            warn_buffer_id_miss("glBindBuffer", target, buffer);
            0
        } else {
            ensure_gles_buffer(buffer)
        };

        if buffer != 0 && gles_id != 0 {
            log::debug!(
                "[FluorateGL] glBindBuffer(0x{:04X}): desktop {} -> GLES {} (tid={})",
                target,
                buffer,
                gles_id,
                state::thread_id_u64()
            );
        }

        (dispatch.bind_buffer)(target, gles_id);

        // 记录 target → desktop buffer ID 映射（供绑定查询/参数 buffer 读取定位）
        state::with_state(|s| {
            s.bound_buffers_by_target.insert(target, buffer);
            if target == 0x8892 || target == 0x8893 {
                s.bound_buffer = buffer;
            }
        });

        // MG（buffer.cpp:524-527 update_vao_ibo_binding）：桌面 GL 语义下
        // ELEMENT_ARRAY_BUFFER 绑定是 VAO state，记录到当前 VAO 名下。
        // glBindVertexArray 恢复绑定记录的配套改动在 vertex_array.rs（跨域协调点）。
        if target == GL_ELEMENT_ARRAY_BUFFER {
            state::with_state(|s| {
                s.element_array_buffer_per_vao
                    .insert(s.bound_vertex_array, buffer);
            });
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferData(
    target: u32,
    size: isize,
    data: *const std::ffi::c_void,
    usage: u32,
) {
    // 诊断：记录所有 glBufferData 调用，确认 Sodium 对哪些 buffer 上传了初始数据
    let bound_desktop = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    log::debug!(
        "[FluorateGL] glBufferData(target=0x{:04X}, size={}, data={}, usage=0x{:04X}, bound_buffer={:?})",
        target,
        size,
        if data.is_null() { "null" } else { "non-null" },
        usage,
        bound_desktop
    );
    // MG 语义（buffer.cpp:1014-1022）：纯透传（GL_PARAMETER_BUFFER 借用
    // GL_COPY_WRITE_BUFFER 代持）+ 记录 size（供 getter 查询）
    with_borrowed_target(target, |dispatch, t| unsafe {
        (dispatch.buffer_data)(t, size, data, usage);
    });
    record_buffer_size(target, size);
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferSubData(
    target: u32,
    offset: isize,
    size: isize,
    data: *const std::ffi::c_void,
) {
    // 诊断：记录所有 glBufferSubData 调用，确认 Sodium 对哪些 buffer 更新了数据
    let bound_desktop = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    log::debug!(
        "[FluorateGL] glBufferSubData(target=0x{:04X}, offset={}, size={}, data={}, bound_buffer={:?})",
        target,
        offset,
        size,
        if data.is_null() { "null" } else { "non-null" },
        bound_desktop
    );
    // MG 语义（buffer.cpp:1026-1032）：纯透传（GL_PARAMETER_BUFFER 借用代持）
    with_borrowed_target(target, |dispatch, t| unsafe {
        (dispatch.buffer_sub_data)(t, offset, size, data);
    });
}

fn is_stub(dispatch: &backend::dispatch::GlesDispatch, f: *const ()) -> bool {
    f == dispatch.stub as *const ()
}

/// 从 GL_PARAMETER_BUFFER 读取实际 draw count（u32），用于模拟 glMultiDraw*IndirectCount。
///
/// MG 语义（buffer.cpp:991-1012 borrowed_target_t）：GL_PARAMETER_BUFFER 无 GLES
/// 绑定槽，借用 GL_COPY_WRITE_BUFFER 临时绑定参数 buffer → `glMapBufferRange(READ_BIT)`
/// 读 4 字节 → `glUnmapBuffer` → 恢复原绑定。
///
/// 返回 None 表示 count buffer 未绑定 / 读取失败，调用方应跳过本次 draw。
pub(crate) fn read_parameter_buffer_u32(offset: isize) -> Option<u32> {
    if offset < 0 {
        return None;
    }
    let desktop_id =
        state::with_state_ref(|s| s.bound_buffers_by_target.get(&GL_PARAMETER_BUFFER).copied())?;
    if desktop_id == 0 {
        return None;
    }
    let gles_id = state::with_state_ref(|s| s.buffers.get_gles(desktop_id))?;

    backend::with_gles_dispatch(|dispatch| unsafe {
        // 借 GL_COPY_WRITE_BUFFER：保存驱动当前绑定 → 绑参数 buffer → 读 → 恢复
        let mut saved: i32 = 0;
        (dispatch.get_integerv)(GL_COPY_WRITE_BUFFER, &mut saved);
        (dispatch.bind_buffer)(GL_COPY_WRITE_BUFFER, gles_id);
        let ptr = (dispatch.map_buffer_range)(GL_COPY_WRITE_BUFFER, offset, 4, GL_MAP_READ_BIT);
        if ptr.is_null() {
            log::warn!(
                "[FluorateGL] read_parameter_buffer_u32: map_range failed (offset={})",
                offset
            );
            (dispatch.bind_buffer)(GL_COPY_WRITE_BUFFER, saved as u32);
            return None;
        }
        let val = std::ptr::read_unaligned(ptr as *const u32);
        (dispatch.unmap_buffer)(GL_COPY_WRITE_BUFFER);
        (dispatch.bind_buffer)(GL_COPY_WRITE_BUFFER, saved as u32);
        log::debug!(
            "[FluorateGL] read_parameter_buffer_u32: COPY_WRITE borrowed read offset={} count={}",
            offset,
            val
        );
        Some(val)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferStorage(
    target: u32,
    size: isize,
    data: *const std::ffi::c_void,
    flags: u32,
) {
    // 诊断：记录所有 glBufferStorage 调用，确认 Sodium 对哪些 buffer 创建了 storage
    let bound_desktop = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    log::debug!(
        "[FluorateGL] glBufferStorage(target=0x{:04X}, size={}, data={}, flags=0x{:04X}, bound_buffer={:?})",
        target,
        size,
        if data.is_null() { "null" } else { "non-null" },
        flags,
        bound_desktop
    );

    let mut gles_flags = flags;
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.buffer_storage as *const ()) {
            // MG（buffer.cpp:1111）：驱动不支持 GL_EXT_buffer_storage → no-op，
            // 不建存储不报错（移除旧版 flags=0 降级 buffer_data 与 malloc shadow 路径）。
            log::debug!(
                "[FluorateGL] glBufferStorage: GL_EXT_buffer_storage unavailable, no-op (size={})",
                size
            );
            return;
        }
        // MG（buffer.cpp:1112-1114）：coherent-as-flush 且带 PERSISTENT 或
        // DYNAMIC_STORAGE 位时追加 WRITE|COHERENT|PERSISTENT（驱动可持久映射的前提）
        if buffer_coherent_as_flush()
            && (gles_flags & GL_MAP_PERSISTENT_BIT != 0 || gles_flags & GL_DYNAMIC_STORAGE_BIT != 0)
        {
            gles_flags |= GL_MAP_WRITE_BIT | GL_MAP_COHERENT_BIT | GL_MAP_PERSISTENT_BIT;
        }
        with_borrowed_target(target, |d, t| {
            (d.buffer_storage)(t, size, data, gles_flags);
        });
    });
    // MG（buffer.cpp:1118）：无论 EXT 是否可用都记录 size
    record_buffer_size(target, size);
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMapBuffer(target: u32, access: u32) -> *mut std::ffi::c_void {
    backend::with_gles_dispatch(|dispatch| {
        // GLES 不提供 glMapBuffer（仅 glMapBufferRange），用 glMapBufferRange 模拟。
        // 若 map_buffer_range 也是 stub（驱动不支持），返回 null 避免后续 UB。
        if is_stub(dispatch, dispatch.map_buffer_range as *const ()) {
            warn_map_buffer_unavailable();
            return std::ptr::null_mut();
        }

        let mut size = 0i32;
        // 包装函数（内部 borrowed_target_t 借用），GL_PARAMETER_BUFFER 安全
        glGetBufferParameteriv(target, 0x8764, &mut size); // GL_BUFFER_SIZE

        // size 为负或零时无意义，直接返回 null
        if size <= 0 {
            log::warn!(
                "[FluorateGL] glMapBuffer: invalid buffer size {}, returning null",
                size
            );
            return std::ptr::null_mut();
        }

        let range_access = match access {
            0x88B8 => GL_MAP_READ_BIT,
            0x88B9 => GL_MAP_WRITE_BIT,
            0x88BA => GL_MAP_READ_BIT | GL_MAP_WRITE_BIT,
            // 其他值按 bit flags 处理，剥离 GLES 不支持的 PERSISTENT/COHERENT
            _ => translate_map_access(access),
        };

        glMapBufferRange(target, 0, size as isize, range_access)
    })
}
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMapBufferRange(
    target: u32,
    offset: isize,
    length: isize,
    access: u32,
) -> *mut std::ffi::c_void {
    // 诊断：记录映射调用（shadow 路径已摘除，MG 语义为直接透传）
    let bound_desktop = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    log::debug!(
        "[FluorateGL] glMapBufferRange(0x{:04X}): GLES native path offset={} length={} access=0x{:04X} bound_buffer={:?}",
        target,
        offset,
        length,
        access,
        bound_desktop
    );
    with_borrowed_target(target, |dispatch, t| unsafe {
        let mut gles_access = access;
        // MG（buffer.cpp:1092）：coherent-as-flush 时映射数据自动对 GPU 可见，
        // 清除 GL_MAP_FLUSH_EXPLICIT_BIT（显式 flush 变 no-op）。
        // PERSISTENT/COHERENT 位不再剥离——直接透传，依赖驱动对
        // GL_EXT_buffer_storage 的真实支持（行为变化点，见报告）。
        if buffer_coherent_as_flush() {
            gles_access &= !GL_MAP_FLUSH_EXPLICIT_BIT;
        }
        (dispatch.map_buffer_range)(t, offset, length, gles_access)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUnmapBuffer(target: u32) -> u8 {
    // MG 语义（buffer.cpp:1098-1107）：纯透传（GL_PARAMETER_BUFFER 借用代持），
    // shadow no-op 分支已摘除
    let bound_desktop = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    log::debug!(
        "[FluorateGL] glUnmapBuffer(0x{:04X}): GLES native path bound_buffer={:?}",
        target,
        bound_desktop
    );
    with_borrowed_target(target, |dispatch, t| unsafe { (dispatch.unmap_buffer)(t) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFlushMappedBufferRange(target: u32, offset: isize, length: isize) {
    // MG（buffer.cpp:1123-1129）：coherent-as-flush 时映射即自动可见，flush 为 no-op
    if buffer_coherent_as_flush() {
        return;
    }
    with_borrowed_target(target, |dispatch, t| unsafe {
        (dispatch.flush_mapped_buffer_range)(t, offset, length);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCopyBufferSubData(
    readTarget: u32,
    writeTarget: u32,
    readOffset: isize,
    writeOffset: isize,
    size: isize,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.copy_buffer_sub_data)(readTarget, writeTarget, readOffset, writeOffset, size);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindBufferBase(target: u32, index: u32, buffer: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // lazy 状态机：同 glBindBuffer——已登记 buffer 首次真实使用（bind）时创建
        let gles_id = if buffer == 0 {
            0
        } else if !state::with_state_ref(|s| s.buffers.contains(buffer)) {
            warn_buffer_id_miss("glBindBufferBase", target, buffer);
            0
        } else {
            ensure_gles_buffer(buffer)
        };

        // P2：atomic counter buffer 绑定转发到 SSBO 绑定点（shader 翻译管线已把
        // atomic_uint 改写为 SSBO，见 preprocess.rs convert_atomic_counter_to_ssbo；
        // GLES 3.1 的 GL_ATOMIC_COUNTER_BUFFER target 本身合法，但 shader 侧
        // 期望的是 binding=N 的 SSBO）。state 记录仍按原 target（查询/同步语义不变）。
        let gles_target = if target == GL_ATOMIC_COUNTER_BUFFER {
            GL_SHADER_STORAGE_BUFFER
        } else {
            target
        };

        (dispatch.bind_buffer_base)(gles_target, index, gles_id);

        // 排查日志：记录 UBO 绑定点调用（MC 若绑定成功，后续 glBufferSubData
        // 才能到达 shader；此调用长期缺失即 UI 消失根因盲区）
        log::debug!(
            "[FluorateGL] glBindBufferBase(target=0x{:04X}, index={}, buffer={}): desktop {} -> GLES {} (tid={})",
            target,
            index,
            buffer,
            buffer,
            gles_id,
            state::thread_id_u64()
        );

        // 记录 target → desktop buffer 映射（供绑定查询/参数 buffer 读取定位）
        state::with_state(|s| {
            s.bound_buffers_by_target.insert(target, buffer);
        });
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindBufferRange(
    target: u32,
    index: u32,
    buffer: u32,
    offset: isize,
    size: isize,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // lazy 状态机：同 glBindBuffer——已登记 buffer 首次真实使用（bind）时创建
        let gles_id = if buffer == 0 {
            0
        } else if !state::with_state_ref(|s| s.buffers.contains(buffer)) {
            warn_buffer_id_miss("glBindBufferRange", target, buffer);
            0
        } else {
            ensure_gles_buffer(buffer)
        };

        // P2：同 glBindBufferBase——atomic counter buffer 转发 SSBO 绑定点
        let gles_target = if target == GL_ATOMIC_COUNTER_BUFFER {
            GL_SHADER_STORAGE_BUFFER
        } else {
            target
        };

        (dispatch.bind_buffer_range)(gles_target, index, gles_id, offset, size);

        // 排查日志：同 glBindBufferBase
        log::debug!(
            "[FluorateGL] glBindBufferRange(target=0x{:04X}, index={}, buffer={}, offset={}, size={}): desktop {} -> GLES {} (tid={})",
            target,
            index,
            buffer,
            offset,
            size,
            buffer,
            gles_id,
            state::thread_id_u64()
        );

        // 记录 target → desktop buffer 映射（供绑定查询/参数 buffer 读取定位）
        state::with_state(|s| {
            s.bound_buffers_by_target.insert(target, buffer);
        });
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferSubData(
    target: u32,
    offset: isize,
    size: isize,
    data: *mut std::ffi::c_void,
) {
    if data.is_null() || size <= 0 || offset < 0 {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.get_buffer_sub_data as *const ()) {
            // GLES 没有 glGetBufferSubData，用 MapBufferRange 模拟
            if is_stub(dispatch, dispatch.map_buffer_range as *const ()) {
                warn_get_buffer_sub_data_unavailable();
                return;
            }
            let ptr = (dispatch.map_buffer_range)(
                target, offset, size, 0x0001, /* GL_MAP_READ_BIT */
            );
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(ptr, data, size as usize);
                (dispatch.unmap_buffer)(target);
            } else {
                // 驱动对源 buffer 拒绝 READ map（GL_EXT_buffer_storage flags=0
                // 的 buffer 无读权限，Mesa 实测）——清掉预期拒绝错误（防止
                // 残留污染宿主 glGetError 序列），借 GL_COPY_READ_BUFFER 中转：
                // copyBufferSubData(target → COPY_READ) → map(COPY_READ) 读 → 恢复。
                let src_gles = state::with_state_ref(|s| {
                    s.bound_buffers_by_target
                        .get(&target)
                        .copied()
                        .and_then(|d| s.buffers.get_gles(d))
                })
                .unwrap_or(0);
                let _ = (dispatch.get_error)(); // 清 map 拒绝的 INVALID_OPERATION
                if src_gles != 0 {
                    let mut saved: i32 = 0;
                    (dispatch.get_integerv)(GL_COPY_READ_BUFFER, &mut saved);
                    let mut tmp: u32 = 0;
                    (dispatch.gen_buffers)(1, &mut tmp);
                    (dispatch.bind_buffer)(GL_COPY_READ_BUFFER, tmp);
                    (dispatch.buffer_data)(GL_COPY_READ_BUFFER, size, std::ptr::null(), 0x88E8);
                    (dispatch.copy_buffer_sub_data)(target, GL_COPY_READ_BUFFER, offset, 0, size);
                    let ptr2 = (dispatch.map_buffer_range)(GL_COPY_READ_BUFFER, 0, size, 0x0001);
                    if !ptr2.is_null() {
                        std::ptr::copy_nonoverlapping(ptr2, data, size as usize);
                        (dispatch.unmap_buffer)(GL_COPY_READ_BUFFER);
                    }
                    (dispatch.delete_buffers)(1, &mut tmp);
                    (dispatch.bind_buffer)(GL_COPY_READ_BUFFER, saved as u32);
                }
            }
        } else {
            (dispatch.get_buffer_sub_data)(target, offset, size, data);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferParameteriv(target: u32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    // MG 语义（buffer.cpp:1034-1040）：借用代持 GL_PARAMETER_BUFFER
    with_borrowed_target(target, |dispatch, t| unsafe {
        (dispatch.get_buffer_parameter_iv)(t, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferPointerv(target: u32, pname: u32, params: *mut *mut std::ffi::c_void) {
    if params.is_null() {
        return;
    }
    // shadow 机制已摘除：GL_BUFFER_MAP_POINTER 直接透传驱动（GLES buffer 真实
    // mapped 状态才返回非 null；MG 无此函数的额外模拟）。
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_buffer_pointer_v)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsBuffer(buffer: u32) -> u8 {
    if buffer == 0 {
        return 0;
    }
    // 桌面 GL 3.3 语义（北极星）：glGenBuffers 生成但尚未 bind/upload 的名称
    // isBuffer 返回 false（GL 3.3 spec §2.9.1）；lazy 状态机下"已创建后端对象"
    // 即 bind 过（ensure_gles_buffer 在 bind/upload 路径创建）→ has_gles 判定
    // 与桌面一致。MG 的"登记即 true"与其 lazy 实现配套，但与桌面语义有偏差，
    // 差分 b01 裁决：按桌面语义修正。
    state::with_state(|s| s.buffers.has_gles(buffer)) as u8
}

// glTexBuffer 将 buffer 绑定到纹理，buffer ID 需要从 desktop 翻译为 GLES。
// GLES 3.1 无 GL_EXT_texture_buffer：MG 以 2D 纹理 + 行式上传模拟
// （buffer.cpp:793-960，emulate_texture_buffer 开关）。

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexBuffer(target: u32, internalformat: u32, buffer: u32) {
    // MG（buffer.cpp:797）：非 TEXTURE_BUFFER target 直接丢弃
    if target != GL_TEXTURE_BUFFER {
        return;
    }

    // lazy 状态机：已登记 buffer 首次真实使用（上传）时创建后端对象
    let gles_id = if buffer == 0 {
        0
    } else if !state::with_state_ref(|s| s.buffers.contains(buffer)) {
        warn_tex_buffer_id_miss("glTexBuffer", target, buffer);
        0
    } else {
        ensure_gles_buffer(buffer)
    };

    log::debug!(
        "[FluorateGL] glTexBuffer(target=0x{:04X}, fmt=0x{:04X}) desktop {} -> GLES {} (tid={})",
        target,
        internalformat,
        buffer,
        gles_id,
        state::thread_id_u64()
    );

    // emulate_texture_buffer 开关（MG gles/loader.cpp:160-162）：GLES <= 3.1 模拟；
    // GLES >= 3.2 走原生 glTexBuffer（驱动无扩展时 stub 检查 + warn，fail-open）
    if !emulate_texture_buffer() {
        backend::with_gles_dispatch(|dispatch| unsafe {
            if is_stub(dispatch, dispatch.tex_buffer as *const ()) {
                warn_tex_buffer_stub("glTexBuffer");
                return;
            }
            (dispatch.tex_buffer)(target, internalformat, gles_id);
        });
        return;
    }

    // ===== 模拟路径（MG buffer.cpp:811-956）=====
    // internalformat 未校验：GL 4.6 table 8.16 之外的格式是 GL_INVALID_ENUM。
    // 先按 texel 大小表拒绝——0 直接让后续 "bufferSize / pixelSize" 除以零，
    // 在 arm64 上产生 0 x 1 纹理，emulated texelFetch 对 0 取模（UB）。
    let pixel_size = get_internal_format_size(internalformat);
    if pixel_size == 0 {
        warn_tex_buffer_emulate_once(
            &TEX_BUFFER_EMULATE_SIZE_WARNED,
            "glTexBuffer: 无 texel 大小表条目，texture buffer 保持不变",
        );
        crate::gl::exports::set_gl_error(GL_INVALID_ENUM);
        return;
    }
    // 硬编码一对 transfer 是历史 bug 来源（除 GL_R8I 外全部 GL_INVALID_OPERATION，
    // level 0 未定义，texelFetch 读零）。按表取 ES 合法 (format, type)。
    let Some((tb_format, tb_type)) = get_internal_format_transfer(internalformat) else {
        warn_tex_buffer_emulate_once(
            &TEX_BUFFER_EMULATE_TRANSFER_WARNED,
            "glTexBuffer: 无 GLES 传输对，texture buffer 保持不变",
        );
        crate::gl::exports::set_gl_error(GL_INVALID_ENUM);
        return;
    };

    backend::with_gles_dispatch(|dispatch| unsafe {
        // unit 15 只借给模拟路径使用，结束后归还
        (dispatch.active_texture)(GL_TEXTURE0 + 15);

        let mut bound_texture: i32 = 0;
        let mut prev_pixel_buffer_binding: i32 = 0;
        (dispatch.get_integerv)(GL_TEXTURE_BINDING_2D, &mut bound_texture);
        (dispatch.get_integerv)(
            GL_PIXEL_UNPACK_BUFFER_BINDING,
            &mut prev_pixel_buffer_binding,
        );

        // 恢复活动 unit：MG 用前端跟踪的 gl_state->current_tex_unit；我们无 unit
        // 跟踪（glActiveTexture 纯透传），用 GL_ACTIVE_TEXTURE 驱动查询（更稳）
        let mut cur_unit: i32 = 0;
        (dispatch.get_integerv)(GL_ACTIVE_TEXTURE, &mut cur_unit);
        let restore_unit = move || {
            (dispatch.active_texture)(cur_unit as u32);
        };

        if bound_texture == 0 {
            // unit 15 上无 2D 纹理——宿主从未把该 buffer texture 绑定为 2D 纹理，
            // 模拟无从下手（MG 同样直接跳过）
            log::debug!("[FluorateGL] glTexBuffer emulate: unit 15 无 2D 纹理，跳过");
            restore_unit();
            return;
        }

        // 读 buffer 大小（经 PIXEL_UNPACK_BUFFER 借用，与 MG 一致）
        (dispatch.bind_buffer)(GL_PIXEL_UNPACK_BUFFER, gles_id);
        let mut buffer_size: i32 = 0;
        (dispatch.get_buffer_parameter_iv)(
            GL_PIXEL_UNPACK_BUFFER,
            GL_BUFFER_SIZE,
            &mut buffer_size,
        );
        (dispatch.bind_buffer)(GL_PIXEL_UNPACK_BUFFER, 0);

        (dispatch.bind_texture)(GL_TEXTURE_2D, bound_texture as u32);

        const MAX_WIDTH: u32 = 8192;
        let num_elements = (buffer_size as u32) / pixel_size as u32;
        if num_elements == 0 {
            // 太小容不下一个 texel：0 x 1 glTexImage2D 会让 emulated texelFetch
            // 对 0 取模（与 pixel_size==0 同源的崩溃路径）
            warn_tex_buffer_emulate_once(
                &TEX_BUFFER_EMULATE_SMALL_WARNED,
                "glTexBuffer: buffer 容不下一个 texel，texture buffer 保持不变",
            );
            crate::gl::exports::set_gl_error(GL_INVALID_VALUE);
            restore_unit();
            return;
        }

        let (width, height) = if num_elements > MAX_WIDTH {
            (MAX_WIDTH, (num_elements + MAX_WIDTH - 1) / MAX_WIDTH)
        } else {
            (num_elements, 1)
        };

        // 保存/清零 unpack 参数（SKIP 不清零会让行式上传偏移错位）
        let mut prev_alignment: i32 = 0;
        let mut prev_row_length: i32 = 0;
        let mut prev_skip_pixels: i32 = 0;
        let mut prev_skip_rows: i32 = 0;
        (dispatch.get_integerv)(GL_UNPACK_ALIGNMENT, &mut prev_alignment);
        (dispatch.get_integerv)(GL_UNPACK_ROW_LENGTH, &mut prev_row_length);
        (dispatch.get_integerv)(GL_UNPACK_SKIP_PIXELS, &mut prev_skip_pixels);
        (dispatch.get_integerv)(GL_UNPACK_SKIP_ROWS, &mut prev_skip_rows);
        (dispatch.pixel_store_i)(GL_UNPACK_SKIP_PIXELS, 0);
        (dispatch.pixel_store_i)(GL_UNPACK_SKIP_ROWS, 0);

        // allocation-only 分配（PBO 绑定前，MG 用 nullptr 分配）
        (dispatch.tex_image_2d)(
            GL_TEXTURE_2D,
            0,
            internalformat as i32,
            width as i32,
            height as i32,
            0,
            tb_format,
            tb_type,
            std::ptr::null(),
        );

        // 行式上传：绑定 PBO 后 tex_sub_image_2d 的 offset 是 buffer 内字节偏移
        (dispatch.bind_buffer)(GL_PIXEL_UNPACK_BUFFER, gles_id);
        for row in 0..height {
            // 最后一行可能不足整行宽——按整行请求会让驱动越过 PBO 末尾读，
            // GLES 报 GL_INVALID_OPERATION 且 no-op，尾部数据永不 upload
            let row_texels = if row + 1 == height {
                num_elements - row * width
            } else {
                width
            };
            if row_texels == 0 {
                break;
            }
            let byte_offset = (row * width * pixel_size as u32) as usize as *const std::ffi::c_void;
            (dispatch.tex_sub_image_2d)(
                GL_TEXTURE_2D,
                0,
                0,
                row as i32,
                row_texels as i32,
                1,
                tb_format,
                tb_type,
                byte_offset,
            );
        }

        (dispatch.pixel_store_i)(GL_UNPACK_ALIGNMENT, prev_alignment);
        (dispatch.pixel_store_i)(GL_UNPACK_ROW_LENGTH, prev_row_length);
        (dispatch.pixel_store_i)(GL_UNPACK_SKIP_PIXELS, prev_skip_pixels);
        (dispatch.pixel_store_i)(GL_UNPACK_SKIP_ROWS, prev_skip_rows);

        // 模拟纹理参数（采样器侧按 2D 采样；MIN/MAG/WRAP 语义对齐 MG）
        (dispatch.tex_parameter_i)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST as i32);
        (dispatch.tex_parameter_i)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST as i32);
        (dispatch.tex_parameter_i)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE as i32);
        (dispatch.tex_parameter_i)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE as i32);
        (dispatch.tex_parameter_i)(GL_TEXTURE_2D, GL_TEXTURE_BASE_LEVEL, 0);
        (dispatch.tex_parameter_i)(GL_TEXTURE_2D, GL_TEXTURE_MAX_LEVEL, 0);

        (dispatch.bind_buffer)(GL_PIXEL_UNPACK_BUFFER, prev_pixel_buffer_binding as u32);
        restore_unit();

        log::debug!(
            "[FluorateGL] glTexBuffer emulate: fmt=0x{:04X} {}x{} texels={} ({} bytes)",
            internalformat,
            width,
            height,
            num_elements,
            buffer_size
        );

        // 跨域协调点：MG 此处还更新纹理对象元数据（mgGetTexObjectByTarget →
        // tex->target/internal_format/width/height/swizzle，texture.cpp:1496 附近
        // 的 TEXTURE_BUFFER 处理依赖它）。我们 texture.rs 的 TEXTURE_META /
        // BOUND_TEXTURES 是私有 thread_local——由域 2（cof-5）补充记录。
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexBufferRange(
    target: u32,
    internalformat: u32,
    buffer: u32,
    offset: isize,
    size: isize,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.tex_buffer_range as *const ()) {
            warn_tex_buffer_stub("glTexBufferRange");
            return;
        }

        // lazy 状态机：已登记 buffer 首次真实使用（上传）时创建后端对象
        let gles_id = if buffer == 0 {
            0
        } else if !state::with_state_ref(|s| s.buffers.contains(buffer)) {
            warn_tex_buffer_id_miss("glTexBufferRange", target, buffer);
            0
        } else {
            ensure_gles_buffer(buffer)
        };

        log::debug!(
            "[FluorateGL] glTexBufferRange(target=0x{:04X}, fmt=0x{:04X}) desktop {} -> GLES {} (tid={})",
            target,
            internalformat,
            buffer,
            gles_id,
            state::thread_id_u64()
        );

        // MG（buffer.cpp:962-979）无 glTexBufferRange 模拟分支：直接透传
        (dispatch.tex_buffer_range)(target, internalformat, gles_id, offset, size);
    });
}
