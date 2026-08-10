//! Draw call 分发与降级
//!
//! 本模块处理非 Multi 的 draw call。Multi-draw 系列见 [`super::multi_draw`]。
//!
//! 决策依据（C1 修订）：**以 dispatch 函数指针存在性（`is_stub`）为主导**决定
//! 原生转发或模拟降级。caps 曾参与判定，但真机能力检测失败（version=0，
//! ANGLE glGetString 返回 null）时 caps 全 false，符号已加载的 3.2 函数被
//! 短路强制降级（BaseVertex 语义丢失）。符号在 = 驱动提供该函数，透传安全；
//! 符号缺失 = 走模拟降级（caps 仍用于 FAKE_EXTENSIONS 剔除等诊断场景）。
//!
//! 降级策略：
//! - `glDrawRangeElements`：不支持时降级为 `glDrawElements`（start/end 是 hint）
//! - `glPrimitiveRestartIndex`：GLES 无此函数，仅记录 restart 索引（对齐
//!   MobileGlues enable.cpp：自定义 restart index 由 draw 前索引流重写模拟）
//! - BaseVertex 系列：不支持时降级为逐索引精确模拟（P1，见
//!   [`draw_elements_basevertex_exact`]）
//! - BaseInstance 系列：不支持时降级为对应 Instanced 版（丢弃 baseinstance）
//! - Indirect 系列：GLES 3.1 core（项目前提），直接转发
//!
//! Primitive restart 集成（D4，对齐 MobileGlues gl/restart.cpp + drawing.cpp）：
//! GLES 只有固定索引重启（GL_PRIMITIVE_RESTART_FIXED_INDEX，逐类型哨兵
//! 0xFF / 0xFFFF / 0xFFFFFFFF），不支持自定义 restart index。draw 前判定：
//! - 应用 restart 索引 == 固定哨兵：驱动 FIXED_INDEX 已生效（见下），零开销直通
//! - 应用 restart 索引 != 固定哨兵：**索引流重写**——map 读索引（或 client
//!   指针）→ 宽化到 u32 + 哨兵替换（+basevertex，哨兵值不偏移）→ 临时 EBO
//!   以 GL_UNSIGNED_INT 重画 → 恢复原绑定
//!
//! 与 MobileGlues 的差异（架构适配）：
//! - MG 用虚拟表跟踪应用 GL_PRIMITIVE_RESTART 启用状态；本项目中 app 的
//!   glEnable(GL_PRIMITIVE_RESTART) 由 exports.rs `translate_enable_cap` 翻译为
//!   GL_PRIMITIVE_RESTART_FIXED_INDEX 直接转发驱动，故用 glIsEnabled 查询驱动
//!   FIXED_INDEX 即应用视角的 restart 状态（每次 draw 一次驱动查询，开销可忽略；
//!   直接 glEnable(0x8D69) 的 GLES 原生 app 同样为 true，与 MG 的
//!   "PRIMITIVE_RESTART || FIXED_INDEX" 取或语义一致）
//! - MG 的重写路径临时 enable/disable 驱动 FIXED_INDEX；我们的驱动 FIXED_INDEX
//!   恒为应用状态（restart 启用即已开启），重写路径**无需 toggle**
//!
//! 持久映射 buffer 同步：`sync_persistent_buffer_if_needed` 已被域 1 删除，
//! 本模块不再有同步调用点（D4 清理）。

use crate::backend;
use crate::backend::dispatch::GlesDispatch;
use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// GL_ELEMENT_ARRAY_BUFFER target
const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
/// GL_PRIMITIVE_RESTART_FIXED_INDEX（GLES 3.0+ core cap；app 的
/// GL_PRIMITIVE_RESTART 由 exports.rs 翻译为此 cap 转发驱动）
const GL_PRIMITIVE_RESTART_FIXED_INDEX: u32 = 0x8D69;
/// GL_UNSIGNED_INT（重写后的索引流宽度）
const GL_UNSIGNED_INT: u32 = 0x1405;

/// 应用侧 primitive restart index（glPrimitiveRestartIndex 记录值）。
///
/// 默认 0（GL 3.3 spec / MG enable.cpp 初始化同款）；glPrimitiveRestartIndex
/// 只记录不转发（GLES 无此函数，驱动恒 stub——MG enable.cpp:684-687 同款）。
static RESTART_INDEX: AtomicU32 = AtomicU32::new(0);

/// 首次告警：restart 索引流重写失败（索引 buffer 不可读）。
static RESTART_REWRITE_READ_FAIL_WARNED: AtomicBool = AtomicBool::new(false);

/// BaseVertex 不支持时的首次告警标志（避免每帧刷屏）
static BASE_VERTEX_WARNED: AtomicBool = AtomicBool::new(false);
/// BaseInstance 不支持时的首次告警标志（避免每帧刷屏）
static BASE_INSTANCE_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：BaseVertex 不可用，降级为普通 draw。
fn warn_base_vertex_unsupported(fname: &str) {
    if !BASE_VERTEX_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 不支持 GL_OES_draw_elements_base_vertex，已降级为普通 draw（索引偏移丢失），后续调用将静默降级",
            fname
        );
    }
}

/// 首次告警：BaseInstance 不可用，降级为对应 Instanced 版。
fn warn_base_instance_unsupported(fname: &str) {
    if !BASE_INSTANCE_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 不支持 GL_EXT_base_instance，已降级为对应 Instanced 版（baseinstance 丢失），后续调用将静默降级",
            fname
        );
    }
}

/// 判断 dispatch 函数指针是否为共享的未实现 stub。
///
/// `load_opt!` / `load_opt_suffixes!` 将缺失的可选函数替换为单个 stub 函数，
/// 故所有 stub 字段地址相同。与 `dispatch.stub` 比较即可识别。
fn is_stub(dispatch: &GlesDispatch, ptr: *const ()) -> bool {
    ptr == dispatch.stub as *const ()
}

/// 索引类型字节宽（0 = 非法/未知类型；MG restart.cpp index_size）。
fn index_size(type_: u32) -> usize {
    match type_ {
        // GL_UNSIGNED_BYTE
        0x1401 => 1,
        // GL_UNSIGNED_SHORT
        0x1403 => 2,
        // GL_UNSIGNED_INT
        0x1405 => 4,
        _ => 0,
    }
}

/// 固定哨兵值（GLES GL_PRIMITIVE_RESTART_FIXED_INDEX 的逐类型重启索引；
/// MG restart.cpp fixed_sentinel）。
fn fixed_sentinel(type_: u32) -> u32 {
    match type_ {
        0x1401 => 0xFF,
        0x1403 => 0xFFFF,
        _ => 0xFFFFFFFF,
    }
}

/// 应用是否要求自定义（非固定哨兵）restart index 且 restart 已启用——
/// 此时索引流必须在 draw 前重写（MG restart.cpp mg_restart_needs_rewrite）。
///
/// 判定用驱动 glIsEnabled(GL_PRIMITIVE_RESTART_FIXED_INDEX)：app 的
/// GL_PRIMITIVE_RESTART 启用经 exports.rs 翻译后必然反映在驱动 FIXED_INDEX
/// 上，两者等价（见模块注释）。
///
/// pub(crate)：multi_draw.rs 的 glMultiDrawElements* 系列共用。
pub(crate) fn restart_needs_rewrite(dispatch: &GlesDispatch, type_: u32) -> bool {
    if index_size(type_) == 0 {
        return false;
    }
    let enabled = unsafe { (dispatch.is_enabled)(GL_PRIMITIVE_RESTART_FIXED_INDEX) != 0 };
    enabled && RESTART_INDEX.load(Ordering::Relaxed) != fixed_sentinel(type_)
}

/// sampler buffer 仿真信息（对齐 MG drawing.cpp SamplerInfo）。
#[derive(Clone)]
struct SamplerInfo {
    /// u_BufferTexWidth uniform location（-1 = 非仿真 program）
    loc_width: i32,
    /// u_BufferTexHeight uniform location
    loc_height: i32,
    /// 程序中 sampler2D/int_sampler2D 类型 uniform 的 location 清单
    samplers: Vec<i32>,
}

// 按（桌面）program id 缓存 sampler 仿真信息。
// 缓存 key 为桌面 program id——由 IdMap 全局唯一单调分配、**永不复用**，
// 因此缓存天然无失效问题（MG 因 GL 会复用程序名字，需要 glCreateProgram/
// glAttachShader 联动清除；我们不需要）。thread_local：与 State 同款，
// GL 上下文线程绑定 → 天然线程/上下文隔离（等效 MG 的 ctx_id 检查）。
thread_local! {
    static SAMPLER_CACHE: std::cell::RefCell<HashMap<u32, Option<SamplerInfo>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// 解析 program 的 sampler buffer 仿真信息（对齐 MG resolve_program）：
/// 1. 查 u_BufferTexWidth uniform——不存在则非仿真 program（-1 快速路径）
/// 2. 枚举 active uniforms 收集 sampler2D / int_sampler2D 类型的 location
///
/// 返回 None = 非仿真 program（也缓存，避免重复枚举）。
fn resolve_sampler_info(dispatch: &GlesDispatch, gles_program: u32) -> Option<SamplerInfo> {
    const GL_ACTIVE_UNIFORMS: u32 = 0x8B86;
    const GL_SAMPLER_2D: u32 = 0x8B5E;
    const GL_INT_SAMPLER_2D: u32 = 0x8DCA;
    const WIDTH_NAME: &[u8] = b"u_BufferTexWidth\0";
    const HEIGHT_NAME: &[u8] = b"u_BufferTexHeight\0";

    let loc_width = unsafe {
        (dispatch.get_uniform_location)(gles_program, WIDTH_NAME.as_ptr() as *const c_char)
    };
    if loc_width == -1 {
        return None;
    }
    let loc_height = unsafe {
        (dispatch.get_uniform_location)(gles_program, HEIGHT_NAME.as_ptr() as *const c_char)
    };

    let mut num_uniforms = 0i32;
    unsafe {
        (dispatch.get_program_iv)(gles_program, GL_ACTIVE_UNIFORMS, &mut num_uniforms);
    }

    let mut samplers = Vec::new();
    let mut name_buf = [0i8; 256];
    for i in 0..num_uniforms {
        let mut length = 0i32;
        let mut size = 0i32;
        let mut utype = 0u32;
        unsafe {
            (dispatch.get_active_uniform)(
                gles_program,
                i as u32,
                256,
                &mut length,
                &mut size,
                &mut utype,
                name_buf.as_mut_ptr() as *mut c_char,
            );
        }
        if utype == GL_SAMPLER_2D || utype == GL_INT_SAMPLER_2D {
            let loc = unsafe {
                (dispatch.get_uniform_location)(gles_program, name_buf.as_ptr() as *const c_char)
            };
            samplers.push(loc);
        }
    }
    Some(SamplerInfo {
        loc_width,
        loc_height,
        samplers,
    })
}

/// 设置 sampler buffer 仿真 uniform（对齐 MG setupBufferTextureUniforms）。
///
/// 仿真纹理（buffer 域 glTexBuffer 在单元 15 上创建的 GL_TEXTURE_2D）已绑定
/// 在纹理单元 15（与 MG kBufferTextureUnit / 我们 buffer 域约定一致）：
/// - 把所有 sampler2D uniform 指向单元 15
/// - 写入 u_BufferTexWidth / u_BufferTexHeight（纹理实际尺寸，驱动查询——
///   MG 从 TextureObject 读，我们没有纹理尺寸 shadow）
///
/// 快速路径：非仿真 program（无 u_BufferTexWidth）在缓存命中后零驱动调用。
fn setup_buffer_texture_uniforms(dispatch: &GlesDispatch, program: u32, gles_program: u32) {
    const BUFFER_TEXTURE_UNIT: i32 = 15;
    const GL_TEXTURE0: u32 = 0x84C0;
    const GL_ACTIVE_TEXTURE: u32 = 0x84E0;
    const GL_TEXTURE_BINDING_2D: u32 = 0x8069;
    const GL_TEXTURE_2D: u32 = 0x0DE1;
    const GL_TEXTURE_WIDTH: u32 = 0x1000;
    const GL_TEXTURE_HEIGHT: u32 = 0x1001;

    // 仿真开关（MG hardware->emulate_texture_buffer）：未启用时不做任何事——
    // 单元 15 属于 app 自己，不能假设它是仿真纹理。
    if !crate::gl::texture::texture_buffer_emulation_enabled() {
        return;
    }

    // 缓存查询（未命中枚举一次；None = 非仿真 program，同样缓存）
    let info_opt: Option<SamplerInfo> = SAMPLER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(v) = cache.get(&program).cloned() {
            return v;
        }
        let v = resolve_sampler_info(dispatch, gles_program);
        cache.insert(program, v.clone());
        v
    });
    let Some(info) = info_opt else {
        return;
    };
    if info.samplers.is_empty() {
        return;
    }

    // 查询单元 15 的 GL_TEXTURE_2D 绑定（驱动查询——我们没有 MG 的纹理绑定
    // shadow；MG 自身也有该 fallback 分支："pay for the round trip"）。
    // 借用的单元必须归还：恢复原 active unit（MG 同款约定）。
    let mut prev_unit = 0i32;
    let mut tex_id = 0i32;
    let mut w = 0i32;
    let mut h = 0i32;
    unsafe {
        (dispatch.get_integerv)(GL_ACTIVE_TEXTURE, &mut prev_unit);
        (dispatch.active_texture)(GL_TEXTURE0 + BUFFER_TEXTURE_UNIT as u32);
        (dispatch.get_integerv)(GL_TEXTURE_BINDING_2D, &mut tex_id);
        if tex_id != 0 {
            (dispatch.get_tex_level_parameter_iv)(GL_TEXTURE_2D, 0, GL_TEXTURE_WIDTH, &mut w);
            (dispatch.get_tex_level_parameter_iv)(GL_TEXTURE_2D, 0, GL_TEXTURE_HEIGHT, &mut h);
        }
        (dispatch.active_texture)(prev_unit as u32);
    }
    if tex_id == 0 {
        return;
    }

    // uniform 写入（MG：采样器 = 单元 15；尺寸 = 纹理宽高）
    for loc in &info.samplers {
        if *loc < 0 {
            continue;
        }
        unsafe {
            (dispatch.uniform_1i)(*loc, BUFFER_TEXTURE_UNIT);
        }
    }
    unsafe {
        (dispatch.uniform_1i)(info.loc_width, w);
        (dispatch.uniform_1i)(info.loc_height, h);
    }
}

/// MG drawing.cpp prepareForDraw 的对应物：draw 前状态准备。
///
/// 唯一动作是 sampler buffer 仿真的 uniform 注入（见
/// [`setup_buffer_texture_uniforms`]）：仅当当前 program 带 u_BufferTexWidth
///（shader 翻译侧注入的仿真 uniform）时才有驱动调用；普通 program 走 -1
/// 快速路径（缓存命中后零开销）。`sync_persistent_buffer_if_needed` 已被
/// 域 1 删除，draw 前无其他准备。
pub(crate) fn prepare_for_draw(dispatch: &GlesDispatch) {
    let (program, gles_program) = crate::state::with_state_ref(|s| {
        let p = s.bound_program;
        let g = s.programs.get_gles(p).unwrap_or(0);
        (p, g)
    });
    if gles_program == 0 {
        return;
    }
    setup_buffer_texture_uniforms(dispatch, program, gles_program);
}

/// MG restart.cpp mg_draw_elements_restart 移植：把应用自定义 restart index
/// 重写为固定哨兵——索引流宽化到 u32（哨兵值保持 0xFFFFFFFF 不偏移
/// +basevertex，GL 4.6 §10.3.6 哨兵比较发生在 basevertex 加法之前；普通索引
/// +basevertex），上传临时 EBO（STREAM_DRAW）以 GL_UNSIGNED_INT 重画，
/// 恢复原 EBO 绑定。
///
/// 返回 true = 重写完成（调用方无需再画）；false = 无法重写（非法索引类型、
/// count<=0、索引 buffer 不可读），调用方按原样 draw（restart 丢失，
/// best-effort——MG 同款策略："hand the batch back so the caller issues its
/// ordinary draw"）。
///
/// `instancecount >= 0` 时走 instanced 重画（MG 同款）；`basevertex` 任意值。
/// 驱动 FIXED_INDEX 已为应用状态（restart 启用即开启），无需临时 toggle。
///
/// pub(crate)：multi_draw.rs 的 glMultiDrawElements* 循环模拟共用（MG 的
/// mg_multidraw_restart_takeover 同款语义：需要重写的 batch 强制逐 draw 重写）。
pub(crate) fn draw_elements_restart_rewrite(
    dispatch: &GlesDispatch,
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    basevertex: i32,
    instancecount: i32,
) -> bool {
    const GL_MAP_READ_BIT: u32 = 0x0001;
    const GL_STREAM_DRAW: u32 = 0x88E0;

    let isize_ = index_size(type_);
    if isize_ == 0 || count <= 0 {
        return false;
    }
    let restart_value = RESTART_INDEX.load(Ordering::Relaxed);
    let n_bytes = (count as usize).saturating_mul(isize_);
    if n_bytes == 0 {
        return false;
    }

    // 读索引源：绑定 EBO → map GLES buffer；无 EBO → client 指针
    // （MG 同款：map 读保证读到 buffer 当前内容，无 shadow 副本可读）
    let ebo_gles = crate::state::with_state_ref(|s| {
        s.bound_buffers_by_target
            .get(&GL_ELEMENT_ARRAY_BUFFER)
            .copied()
            .and_then(|d| s.buffers.get_gles(d))
    })
    .unwrap_or(0);

    let mut src: Vec<u8> = Vec::with_capacity(n_bytes);
    unsafe {
        if ebo_gles != 0 {
            let ptr = (dispatch.map_buffer_range)(
                GL_ELEMENT_ARRAY_BUFFER,
                indices as isize,
                n_bytes as isize,
                GL_MAP_READ_BIT,
            );
            if ptr.is_null() {
                // 不可读的索引 buffer（glBufferStorage 等）无法重写：MG 同款
                // 策略——放弃重写让调用方原样 draw（restart 丢失但几何不丢，
                // "nothing is invented"）。
                if !RESTART_REWRITE_READ_FAIL_WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!(
                        "[FluorateGL] primitive restart: 索引 buffer 不可读（map 失败），自定义 restart index 忽略"
                    );
                }
                return false;
            }
            std::ptr::copy_nonoverlapping(ptr as *const u8, src.as_mut_ptr(), n_bytes);
            src.set_len(n_bytes);
            (dispatch.unmap_buffer)(GL_ELEMENT_ARRAY_BUFFER);
        } else if indices.is_null() {
            if !RESTART_REWRITE_READ_FAIL_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!("[FluorateGL] primitive restart: 无 EBO 绑定且 indices 为空，无法重写");
            }
            return false;
        } else {
            std::ptr::copy_nonoverlapping(indices as *const u8, src.as_mut_ptr(), n_bytes);
            src.set_len(n_bytes);
        }
    }

    // 宽化重写（MG restart.cpp rewrite：v == restart_value → 0xFFFFFFFF，
    // 否则 v + basevertex；u32 wrapping 加法）
    let bv = basevertex as u32;
    let mut rewritten: Vec<u32> = Vec::with_capacity(count as usize);
    match type_ {
        0x1401 => {
            for &v in src.iter() {
                let v = v as u32;
                rewritten.push(if v == restart_value {
                    0xFFFFFFFF
                } else {
                    v.wrapping_add(bv)
                });
            }
        }
        0x1403 => {
            for chunk in src.chunks_exact(2) {
                let v = u16::from_le_bytes([chunk[0], chunk[1]]) as u32;
                rewritten.push(if v == restart_value {
                    0xFFFFFFFF
                } else {
                    v.wrapping_add(bv)
                });
            }
        }
        0x1405 => {
            for chunk in src.chunks_exact(4) {
                let v = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                rewritten.push(if v == restart_value {
                    0xFFFFFFFF
                } else {
                    v.wrapping_add(bv)
                });
            }
        }
        _ => unreachable!(), // index_size 已校验
    }

    // 临时 EBO 重画（MG：scratch buffer + GL_UNSIGNED_INT + nullptr offset）
    unsafe {
        let mut tmp_buf: u32 = 0;
        (dispatch.gen_buffers)(1, &mut tmp_buf);
        (dispatch.bind_buffer)(GL_ELEMENT_ARRAY_BUFFER, tmp_buf);
        (dispatch.buffer_data)(
            GL_ELEMENT_ARRAY_BUFFER,
            (rewritten.len() * 4) as isize,
            rewritten.as_ptr() as *const std::ffi::c_void,
            GL_STREAM_DRAW,
        );
        if instancecount >= 0 {
            (dispatch.draw_elements_instanced)(
                mode,
                count,
                GL_UNSIGNED_INT,
                std::ptr::null(),
                instancecount,
            );
        } else {
            (dispatch.draw_elements)(mode, count, GL_UNSIGNED_INT, std::ptr::null());
        }
        (dispatch.delete_buffers)(1, &mut tmp_buf);
        (dispatch.bind_buffer)(GL_ELEMENT_ARRAY_BUFFER, ebo_gles);
    }
    true
}

/// BaseVertex 精确降级：逐索引加法（P1 修复，对齐 MobileGlues drawing.cpp:160-247）。
///
/// 旧实现 offset_indices 用指针偏移（indices + basevertex×type_size），
/// 其等价性仅对顺序索引成立：`indices[i]+bv == indices[i+bv]` 需索引值
/// 连续递增。乱序/重复索引的 EBO 下两者不等价（如 EBO={0,5,2} + bv=3，
/// 正确=顶点{3,8,5}，指针偏移却读 EBO[3..]）→ 画错。
///
/// 本实现精确模拟 GL 3.3 语义"实际索引 = 索引值 + basevertex"：
/// 1. 读索引：绑定 EBO 时 map 读；client 指针（无 EBO）直接拷贝
/// 2. 逐索引 +basevertex（u32 wrapping——索引越界是 app 错误，wrap 无害，
///    与 MobileGlues 直接加法一致）
/// 3. 写入临时 EBO（STREAM_DRAW）重画，恢复原绑定
///
/// `instancecount = Some(n)` 时走 instanced 版本（同 MobileGlues 无此路径，
/// 我们为 glDrawElementsInstancedBaseVertex 家族复用）。
/// 任何失败路径 best-effort 回退原 draw（与旧行为一致的降级），不崩溃。
///
/// 注：本路径不含 restart 哨兵处理（MG 同款缺陷——restart 重写优先分支已在
/// 调用方处理；走到此处时 index==固定哨兵 或 restart 未启用，哨兵会被
/// +basevertex 偏移，MG drawing.cpp 模拟路径同样如此）。
///
/// pub(crate)：multi_draw.rs 的 glMultiDrawElementsBaseVertex 第三级降级共用
/// （P1：MultiDraw 逐 draw 精确模拟与单 draw 版行为对齐）。
pub(crate) fn draw_elements_basevertex_exact(
    dispatch: &GlesDispatch,
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    basevertex: i32,
    instancecount: Option<i32>,
) {
    if count <= 0 {
        return;
    }
    // 快速路径：basevertex=0 语义等同普通 draw（零转换开销）
    if basevertex == 0 {
        unsafe {
            match instancecount {
                Some(ic) => (dispatch.draw_elements_instanced)(mode, count, type_, indices, ic),
                None => (dispatch.draw_elements)(mode, count, type_, indices),
            }
        }
        return;
    }

    const GL_MAP_READ_BIT: u32 = 0x0001;
    const GL_STREAM_DRAW: u32 = 0x88E0;

    let index_size = index_size(type_);
    if index_size == 0 {
        log::error!(
            "[FluorateGL] draw_elements_basevertex_exact: 未知索引类型 0x{:04X}，无法精确模拟，按原 draw 降级",
            type_
        );
        unsafe {
            match instancecount {
                Some(ic) => (dispatch.draw_elements_instanced)(mode, count, type_, indices, ic),
                None => (dispatch.draw_elements)(mode, count, type_, indices),
            }
        }
        return;
    }
    let n_bytes = (count as usize).saturating_mul(index_size);
    if n_bytes == 0 {
        return;
    }

    // 读索引源：绑定 EBO → map GLES buffer；无 EBO → client 指针
    let ebo_gles = crate::state::with_state_ref(|s| {
        s.bound_buffers_by_target
            .get(&GL_ELEMENT_ARRAY_BUFFER)
            .copied()
            .and_then(|d| s.buffers.get_gles(d))
    })
    .unwrap_or(0);

    let mut temp: Vec<u8> = Vec::with_capacity(n_bytes);
    unsafe {
        if ebo_gles != 0 {
            let src = (dispatch.map_buffer_range)(
                GL_ELEMENT_ARRAY_BUFFER,
                indices as isize,
                n_bytes as isize,
                GL_MAP_READ_BIT,
            );
            if src.is_null() {
                log::warn!(
                    "[FluorateGL] draw_elements_basevertex_exact: EBO map 失败（offset=0x{:x}），按原 draw best-effort 降级",
                    indices as usize
                );
                match instancecount {
                    Some(ic) => (dispatch.draw_elements_instanced)(mode, count, type_, indices, ic),
                    None => (dispatch.draw_elements)(mode, count, type_, indices),
                }
                return;
            }
            std::ptr::copy_nonoverlapping(src as *const u8, temp.as_mut_ptr(), n_bytes);
            (dispatch.unmap_buffer)(GL_ELEMENT_ARRAY_BUFFER);
        } else {
            // client 指针（无 EBO 绑定）
            std::ptr::copy_nonoverlapping(indices as *const u8, temp.as_mut_ptr(), n_bytes);
        }
    }

    // 逐索引 +basevertex（u32 wrapping，负 basevertex 由宿主保证索引+bv ≥ 0）
    match type_ {
        0x1401 => {
            for v in temp.iter_mut() {
                *v = v.wrapping_add(basevertex as u8);
            }
        }
        0x1403 => {
            for chunk in temp.chunks_exact_mut(2) {
                let v = u16::from_le_bytes([chunk[0], chunk[1]]).wrapping_add(basevertex as u16);
                let b = v.to_le_bytes();
                chunk[0] = b[0];
                chunk[1] = b[1];
            }
        }
        0x1405 => {
            for chunk in temp.chunks_exact_mut(4) {
                let v = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    .wrapping_add(basevertex as u32);
                let b = v.to_le_bytes();
                chunk.copy_from_slice(&b);
            }
        }
        _ => unreachable!(), // 上面已校验
    }

    // 临时 EBO 重画（MobileGlues 同款：gen → bind → data → draw → delete → 恢复）
    unsafe {
        let mut tmp_buf: u32 = 0;
        (dispatch.gen_buffers)(1, &mut tmp_buf);
        (dispatch.bind_buffer)(GL_ELEMENT_ARRAY_BUFFER, tmp_buf);
        (dispatch.buffer_data)(
            GL_ELEMENT_ARRAY_BUFFER,
            n_bytes as isize,
            temp.as_ptr() as *const std::ffi::c_void,
            GL_STREAM_DRAW,
        );
        match instancecount {
            Some(ic) => {
                (dispatch.draw_elements_instanced)(mode, count, type_, std::ptr::null(), ic)
            }
            None => (dispatch.draw_elements)(mode, count, type_, std::ptr::null()),
        }
        (dispatch.delete_buffers)(1, &mut tmp_buf);
        (dispatch.bind_buffer)(GL_ELEMENT_ARRAY_BUFFER, ebo_gles);
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawRangeElements(
    mode: u32,
    start: u32,
    end: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
) {
    // GL 3.3 §2.8.1：end < start → GL_INVALID_VALUE（须在 restart 重写前检查——
    // 重写会丢弃 start/end，导致桌面报错场景被吞）
    if end < start {
        crate::gl::exports::inject_gl_error(0x0501); // GL_INVALID_VALUE
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先。重写后的流是 32 位 + 0xFFFFFFFF 哨兵，
        // start/end 不再描述它——它们只是索引范围 promise，丢弃允许
        // （MG drawing.cpp 同款注释："dropping the promise is allowed;
        // drawing the wrong primitives is not"）。
        if restart_needs_rewrite(dispatch, type_)
            && draw_elements_restart_rewrite(dispatch, mode, count, type_, indices, 0, -1)
        {
            return;
        }
        if is_stub(dispatch, dispatch.draw_range_elements as *const ()) {
            // GLES 不支持 glDrawRangeElements 时降级为 glDrawElements
            // start/end 只是 hint，跳过它们不影响正确性
            log::debug!(
                "[FluorateGL] glDrawRangeElements fallback to glDrawElements (stub detected)"
            );
            (dispatch.draw_elements)(mode, count, type_, indices);
        } else {
            (dispatch.draw_range_elements)(mode, start, end, count, type_, indices);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysInstanced(mode: u32, first: i32, count: i32, instancecount: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        (dispatch.draw_arrays_instanced)(mode, first, count, instancecount);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsInstanced(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    instancecount: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先（MG drawing.cpp:203-214 同款结构）
        if restart_needs_rewrite(dispatch, type_)
            && draw_elements_restart_rewrite(
                dispatch,
                mode,
                count,
                type_,
                indices,
                0,
                instancecount,
            )
        {
            return;
        }
        (dispatch.draw_elements_instanced)(mode, count, type_, indices, instancecount);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPrimitiveRestartIndex(index: u32) {
    // D4：只记录不转发（对齐 MG enable.cpp：GLES 无 glPrimitiveRestartIndex，
    // 驱动恒 stub；自定义索引由 draw 前索引流重写模拟）。默认 0（GL 3.3
    // spec §10.3.6 / MG enable.cpp:285 同款）。
    // 跨域闭环（域 6）：同步写 enable_state 表——宿主 glGetIntegerv
    // (GL_PRIMITIVE_RESTART_INDEX) 查询返回实际值（MG enable.cpp 同款）。
    RESTART_INDEX.store(index, Ordering::Relaxed);
    crate::gl::exports::enable_state::set_primitive_restart_index(index);
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsBaseVertex(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    basevertex: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先（重写同时应用 basevertex，覆盖驱动支持与
        // 模拟两条路径——MG drawing.cpp:300-307 同款注释）
        if restart_needs_rewrite(dispatch, type_)
            && draw_elements_restart_rewrite(dispatch, mode, count, type_, indices, basevertex, -1)
        {
            return;
        }
        // C1：supported 以 dispatch 符号存在性为主导（is_stub 兜底）。
        // caps 曾参与判定——真机能力检测失败（version=0）时符号已加载也被
        // 短路强制降级，导致 basevertex 语义丢失（Sodium 渲染错误风险）。
        let supported = !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
        if !supported {
            // 降级：P1 逐索引加法精确模拟（读索引 + basevertex → 临时 EBO 重画）。
            // 旧实现指针偏移仅对顺序索引等价，乱序 EBO 画错。
            warn_base_vertex_unsupported("glDrawElementsBaseVertex");
            draw_elements_basevertex_exact(dispatch, mode, count, type_, indices, basevertex, None);
        } else {
            (dispatch.draw_elements_base_vertex)(mode, count, type_, indices, basevertex);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawRangeElementsBaseVertex(
    mode: u32,
    start: u32,
    end: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    basevertex: i32,
) {
    // D1：GL 3.3.1 core 导出补齐（此前 dispatch 有字段/加载但无导出符号，
    // LWJGL 绑定 null 有崩溃风险）。三级降级（同 basevertex 家族模式）：
    // 透传（GLES 3.2 core 同名）→ glDrawElementsBaseVertex（start/end 是 hint）
    // → P1 逐索引加法精确模拟 + glDrawElements
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先（MG drawing.cpp:465-470 同款）
        if restart_needs_rewrite(dispatch, type_)
            && draw_elements_restart_rewrite(dispatch, mode, count, type_, indices, basevertex, -1)
        {
            return;
        }
        // C1：以符号存在性为主导
        let supported = !is_stub(
            dispatch,
            dispatch.draw_range_elements_base_vertex as *const (),
        );
        if !supported {
            // 二级：降级为 glDrawElementsBaseVertex（start/end 是 hint，跳过不影响
            // 正确性——同 glDrawRangeElements 降级策略）
            let base_vertex_ok =
                !is_stub(dispatch, dispatch.draw_elements_base_vertex as *const ());
            if !base_vertex_ok {
                // 三级：P1 逐索引加法精确模拟
                warn_base_vertex_unsupported("glDrawRangeElementsBaseVertex");
                draw_elements_basevertex_exact(
                    dispatch, mode, count, type_, indices, basevertex, None,
                );
            } else {
                (dispatch.draw_elements_base_vertex)(mode, count, type_, indices, basevertex);
            }
        } else {
            (dispatch.draw_range_elements_base_vertex)(
                mode, start, end, count, type_, indices, basevertex,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysIndirect(mode: u32, indirect: *const std::ffi::c_void) {
    log::debug!("[FluorateGL] glDrawArraysIndirect(mode=0x{:04X})", mode);
    // GLES 3.1 core 特性，项目前提，直接转发（MG gl_native.cpp 同款纯透传；
    // D4 已移除 sync_persistent_buffer_if_needed——域 1 删除）
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        (dispatch.draw_arrays_indirect)(mode, indirect);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsIndirect(mode: u32, type_: u32, indirect: *const std::ffi::c_void) {
    log::debug!(
        "[FluorateGL] glDrawElementsIndirect(mode=0x{:04X}, type=0x{:04X})",
        mode,
        type_
    );
    // GLES 3.1 core 特性，项目前提，直接转发（MG gl_native.cpp 同款纯透传；
    // MG 的单次 indirect 无 restart 处理——命令流在 GPU buffer 中不可重写）
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        (dispatch.draw_elements_indirect)(mode, type_, indirect);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArraysInstancedBaseInstance(
    mode: u32,
    first: i32,
    count: i32,
    instancecount: i32,
    baseinstance: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // C1：以符号存在性为主导（caps 误判 false 时不再强制降级）。
        // MG 对齐：baseinstance!=0 时告警一次（我们降级路径有同款告警；
        // 透传路径驱动支持则无此问题）
        let supported = !is_stub(
            dispatch,
            dispatch.draw_arrays_instanced_base_instance as *const (),
        );
        if !supported {
            // 降级为 glDrawArraysInstanced，丢弃 baseinstance。
            // 影响：使用 instance ID 计算属性偏移的 shader 会错位，仅 best-effort。
            warn_base_instance_unsupported("glDrawArraysInstancedBaseInstance");
            (dispatch.draw_arrays_instanced)(mode, first, count, instancecount);
        } else {
            (dispatch.draw_arrays_instanced_base_instance)(
                mode,
                first,
                count,
                instancecount,
                baseinstance,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsInstancedBaseInstance(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    instancecount: i32,
    baseinstance: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先（baseinstance 在重写路径丢失——MG 同款：
        // MG 的 glDrawElementsInstancedBaseInstance 转发到
        // glDrawElementsInstanced，重写由后者处理，同样无 baseinstance）
        if restart_needs_rewrite(dispatch, type_)
            && draw_elements_restart_rewrite(
                dispatch,
                mode,
                count,
                type_,
                indices,
                0,
                instancecount,
            )
        {
            return;
        }
        // C1：以符号存在性为主导（caps 误判 false 时不再强制降级）
        let supported = !is_stub(
            dispatch,
            dispatch.draw_elements_instanced_base_instance as *const (),
        );
        if !supported {
            warn_base_instance_unsupported("glDrawElementsInstancedBaseInstance");
            (dispatch.draw_elements_instanced)(mode, count, type_, indices, instancecount);
        } else {
            (dispatch.draw_elements_instanced_base_instance)(
                mode,
                count,
                type_,
                indices,
                instancecount,
                baseinstance,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsInstancedBaseVertex(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    instancecount: i32,
    basevertex: i32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先（MG drawing.cpp:482-490 同款：重写同时应用
        // basevertex + instancecount）
        if restart_needs_rewrite(dispatch, type_)
            && draw_elements_restart_rewrite(
                dispatch,
                mode,
                count,
                type_,
                indices,
                basevertex,
                instancecount,
            )
        {
            return;
        }
        // C1：以符号存在性为主导（caps 误判 false 时不再强制降级）
        let supported = !is_stub(
            dispatch,
            dispatch.draw_elements_instanced_base_vertex as *const (),
        );
        if !supported {
            // 降级：P1 逐索引加法精确模拟（保留 instancecount）
            warn_base_vertex_unsupported("glDrawElementsInstancedBaseVertex");
            draw_elements_basevertex_exact(
                dispatch,
                mode,
                count,
                type_,
                indices,
                basevertex,
                Some(instancecount),
            );
        } else {
            (dispatch.draw_elements_instanced_base_vertex)(
                mode,
                count,
                type_,
                indices,
                instancecount,
                basevertex,
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElementsInstancedBaseVertexBaseInstance(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
    instancecount: i32,
    basevertex: i32,
    baseinstance: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        prepare_for_draw(dispatch);
        // D4：restart 重写优先（basevertex + instancecount 保留，baseinstance
        // 丢失——MG 同款：转发链上重写路径无 baseinstance）
        if restart_needs_rewrite(dispatch, type_)
            && draw_elements_restart_rewrite(
                dispatch,
                mode,
                count,
                type_,
                indices,
                basevertex,
                instancecount,
            )
        {
            return;
        }
        // C1：以符号存在性为主导（caps 误判 false 时不再强制降级）。
        // 该函数需要 base_vertex + base_instance 两个特性——符号在即两个都可用。
        let supported = !is_stub(
            dispatch,
            dispatch.draw_elements_instanced_base_vertex_base_instance as *const (),
        );
        if !supported {
            // 同时丢失 basevertex 和 baseinstance，触发两类首次告警；
            // basevertex 用 P1 逐索引加法精确模拟，baseinstance 无法补偿
            warn_base_vertex_unsupported("glDrawElementsInstancedBaseVertexBaseInstance");
            warn_base_instance_unsupported("glDrawElementsInstancedBaseVertexBaseInstance");
            draw_elements_basevertex_exact(
                dispatch,
                mode,
                count,
                type_,
                indices,
                basevertex,
                Some(instancecount),
            );
        } else {
            (dispatch.draw_elements_instanced_base_vertex_base_instance)(
                mode,
                count,
                type_,
                indices,
                instancecount,
                basevertex,
                baseinstance,
            );
        }
    });
}

/// GL 3.3 core §4.1：glDrawTransformFeedback / glDrawTransformFeedbackInstanced。
///
/// D2：导出补齐（此前缺失——LWJGL 绑定 null 有崩溃风险）。GLES 3.1 无对应
/// 函数（transform feedback 捕获回读绘制在 GLES 中不存在），语义无法模拟，
/// 故 stub no-op + 首次调用告警。调用方通常先查询扩展/版本再使用，实际
/// 触发概率低；导出符号存在即可避免 LWJGL 层 null 崩溃。
static TF_DRAW_WARNED: AtomicBool = AtomicBool::new(false);

fn warn_tf_draw_unsupported(fname: &str) {
    if !TF_DRAW_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 无 transform feedback 回读绘制（glDrawTransformFeedback），已 no-op，后续调用将静默跳过",
            fname
        );
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawTransformFeedback(_mode: u32, _id: u32) {
    warn_tf_draw_unsupported("glDrawTransformFeedback");
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawTransformFeedbackInstanced(_mode: u32, _id: u32, _instancecount: i32) {
    warn_tf_draw_unsupported("glDrawTransformFeedbackInstanced");
}

/// GL_SHADER_STORAGE_BARRIER_BIT（glMemoryBarrier 补位用；注意不是 target 值 0x90F2）
const GL_SHADER_STORAGE_BARRIER_BIT: u32 = 0x00002000;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDispatchCompute(num_groups_x: u32, num_groups_y: u32, num_groups_z: u32) {
    // P2：compute 分发的运行时载体（GLES 3.1 core 透传，MG drawing.cpp:244-250
    // 同款无 prepare 纯透传）。
    // 依赖：shader 翻译管线已把 atomic_uint 改写为 SSBO；app 的
    // GL_ATOMIC_COUNTER_BUFFER 绑定在 glBindBufferBase/Range 时已转发到
    // GL_SHADER_STORAGE_BUFFER（见 buffer.rs），此处无需额外处理。
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.dispatch_compute)(num_groups_x, num_groups_y, num_groups_z);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMemoryBarrier(barriers: u32) {
    // P2：补 GL_SHADER_STORAGE_BARRIER_BIT——atomic→SSBO 模拟后跨 dispatch 的
    // 可见性依赖 SSBO barrier（对齐 MobileGlues drawing.cpp glMemoryBarrier；
    // OR 操作无副作用。MG 为纯透传，我们补位是 P2 模拟的显式需求）。
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.memory_barrier)(barriers | GL_SHADER_STORAGE_BARRIER_BIT);
    });
}

/// glBindImageTexture（D4-3 真实化：MG drawing.cpp:228-235 同款透传，
/// 替代 stub_exports.rs 的 stub 条目——生成器协调见 symbols.rs 补登记）。
///
/// GLES 3.1 core 函数（shader image load/store 的运行时载体）。dispatch
/// 字段为 load_opt（旧驱动缺失时安全网）：stub → no-op + 首次告警
/// （fail-open，不注入错误——image 绑定缺失只影响使用 image 的 shader，
/// MC 主渲染路径不用）。
static BIND_IMAGE_TEXTURE_WARNED: AtomicBool = AtomicBool::new(false);

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindImageTexture(
    unit: u32,
    texture: u32,
    level: i32,
    layered: u8,
    layer: i32,
    access: u32,
    format: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.bind_image_texture as *const ()) {
            if !BIND_IMAGE_TEXTURE_WARNED.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "[FluorateGL] glBindImageTexture: 驱动不支持（stub），image 绑定被忽略（后续调用将静默跳过）"
                );
            }
            return;
        }
        (dispatch.bind_image_texture)(unit, texture, level, layered, layer, access, format);
    });
}
