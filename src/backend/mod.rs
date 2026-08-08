//! EGL/GLES 后端加载与函数指针分发
//!
//! 职责：
//! - 按配置（`Config::backend`）dlopen 对应平台的 EGL/GLES 库
//! - 通过 `load_opt!` 宏用 dlsym 加载函数指针，缺失的可选函数替换为 stub
//! - 提供 `with_gles_dispatch` / `with_egl_dispatch` 给拦截层调用
//!
//! 全局状态用 `OnceLock` 存储，库生命周期内不可变；首次 GL/EGL 调用时
//! 触发 GPU 信息记录与驱动 Debug 噪声屏蔽。

pub mod capabilities;
pub mod dispatch;
pub mod loader;

use crate::config::Config;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

// 全局状态
static CONFIG: OnceLock<Config> = OnceLock::new();
pub static GLES_DISPATCH: OnceLock<dispatch::GlesDispatch> = OnceLock::new();
static EGL_DISPATCH: OnceLock<crate::egl_sys::dispatch::EglDispatch> = OnceLock::new();
static INIT_ONCE: OnceLock<()> = OnceLock::new();
static GLES_CAPABILITIES: OnceLock<capabilities::GlesCapabilities> = OnceLock::new();

/// 标记后端初始化完成。
/// 惰性初始化路径（init::ensure_backend_initialized）内部调用；
/// 当前仅 set 不被读取，保留以兼容旧调用方（历史兼容保留）。
pub fn mark_initialized() {
    let _ = INIT_ONCE.set(());
}

pub fn set_config(config: Config) {
    let _ = CONFIG.set(config);
}

/// 返回当前生效的后端配置。
/// CONFIG 未设置时（理论上 ctor 必先执行）提供 from_env 兜底，保证惰性初始化可用。
pub fn config() -> &'static Config {
    CONFIG.get_or_init(Config::from_env)
}

pub fn init_gles() -> Result<(), &'static str> {
    let config = CONFIG.get().expect("config not set");

    let loader = loader::GlesLoader::new(config)?;
    let dispatch = dispatch::GlesDispatch::load_from(&loader)
        .ok_or("failed to load required GLES function")?;

    let _ = GLES_DISPATCH.set(dispatch);

    Box::leak(Box::new(loader));
    Ok(())
}

pub fn init_egl() -> Result<(), &'static str> {
    let config = CONFIG.get().expect("config not set");

    let loader = crate::egl_sys::loader::EglLoader::new(config)?;
    let dispatch = crate::egl_sys::dispatch::EglDispatch::load_from(&loader)
        .ok_or("failed to load required EGL function")?;

    let _ = EGL_DISPATCH.set(dispatch);

    Box::leak(Box::new(loader));
    Ok(())
}

pub fn with_gles_dispatch<F, R>(f: F) -> R
where
    F: FnOnce(&dispatch::GlesDispatch) -> R,
{
    crate::init::ensure_backend_initialized(); // 首调惰性初始化（init_gles 内部不走 with_*，无递归）
    static FIRST_CALL: AtomicBool = AtomicBool::new(true);
    if FIRST_CALL.swap(false, Ordering::Relaxed) {
        log::info!("[FluorateGL] === 首次 GL 调用，游戏渲染管线已启动 ===");
        suppress_debug_noise();
        log_gpu_info();
        query_capabilities_now();
    }

    let dispatch = GLES_DISPATCH.get().unwrap_or_else(|| {
        static STUB: OnceLock<dispatch::GlesDispatch> = OnceLock::new();
        STUB.get_or_init(dispatch::GlesDispatch::all_stub)
    });
    f(dispatch)
}

/// 在首次 GL 调用时（EGL 上下文已创建）查询并记录 GPU 信息。
/// 不能在 `fluorategl_init()` 中查询，因为那时 EGL 上下文尚未创建。
fn log_gpu_info() {
    use libc::c_char;
    unsafe fn c_str_to_string(ptr: *const c_char) -> String {
        if ptr.is_null() {
            return "(null)".to_string();
        }
        unsafe {
            std::ffi::CStr::from_ptr(ptr as *const _)
                .to_string_lossy()
                .into_owned()
        }
    }

    if let Some(dispatch) = GLES_DISPATCH.get() {
        unsafe {
            let version = (dispatch.get_string)(0x1F02); // GL_VERSION
            let renderer = (dispatch.get_string)(0x1F01); // GL_RENDERER
            let vendor = (dispatch.get_string)(0x1F00); // GL_VENDOR

            log::info!("[FluorateGL] GLES version: {}", c_str_to_string(version));
            log::info!("[FluorateGL] GPU: {}", c_str_to_string(renderer));
            log::info!("[FluorateGL] Vendor: {}", c_str_to_string(vendor));
        }
    }
}

/// 屏蔽 GLES 驱动的 PERFORMANCE / OTHER 类型 Debug 消息，
/// 避免 "Packing allocations" / "high level of unsubmitted work" 等刷屏。
///
/// 两道防线：
/// 1. glDebugMessageControl 关闭 PERFORMANCE / OTHER 类型（保留 ERROR 用于诊断）；
/// 2. glDisable(GL_DEBUG_OUTPUT) 彻底关闭 KHR_debug 回调输出。
///
/// 部分Adreno 驱动无视 glDebugMessageControl 对 HIGH 级 PERFORMANCE 消息的过滤，
/// 故必须配合 glDisable(GL_DEBUG_OUTPUT)。MC(blaze3d) 会在上下文初始化后自行
/// glEnable(GL_DEBUG_OUTPUT) 注册回调，因此拦截层 exports::glEnable 也会吞掉
/// GL_DEBUG_OUTPUT，防止其被重新启用（见 exports.rs）。
fn suppress_debug_noise() {
    const GL_DONT_CARE: u32 = 0x1100;
    const GL_DEBUG_OUTPUT: u32 = 0x9146;
    const GL_DEBUG_TYPE_PERFORMANCE: u32 = 0x8250;
    const GL_DEBUG_TYPE_OTHER: u32 = 0x8251;
    const GL_FALSE: u8 = 0;

    let dispatch = GLES_DISPATCH.get().unwrap_or_else(|| {
        static STUB: OnceLock<dispatch::GlesDispatch> = OnceLock::new();
        STUB.get_or_init(dispatch::GlesDispatch::all_stub)
    });

    // 防线 1：按类型屏蔽（仅当驱动支持 glDebugMessageControl 时）
    if dispatch.debug_message_control as *const () != dispatch.stub as *const () {
        unsafe {
            (dispatch.debug_message_control)(
                GL_DONT_CARE,
                GL_DEBUG_TYPE_PERFORMANCE,
                GL_DONT_CARE,
                0,
                std::ptr::null(),
                GL_FALSE,
            );
            (dispatch.debug_message_control)(
                GL_DONT_CARE,
                GL_DEBUG_TYPE_OTHER,
                GL_DONT_CARE,
                0,
                std::ptr::null(),
                GL_FALSE,
            );
        }
    }

    // 防线 2：彻底关闭 GL_DEBUG_OUTPUT，阻止驱动通过 KHR_debug 回调输出任何消息。
    // 直接走 dispatch（绕过拦截层 exports::glDisable），避免被任何上层逻辑干扰。
    unsafe {
        (dispatch.disable)(GL_DEBUG_OUTPUT);
        // 部分驱动（如 llvmpipe GLES 3.2）不支持 GL_DEBUG_OUTPUT 作为 enable cap，
        // 直通产生 INVALID_ENUM 并污染宿主 glGetError 队列（差分测试 a00/e08/g10
        // 实测 0x0500 残留）。首次 GL 调用时刻错误队列必为空（此前无任何 GL 调用），
        // 立即弹出该预期错误安全且不影响宿主错误检测。
        let _ = (dispatch.get_error)();
    }

    log::info!(
        "[FluorateGL] 已屏蔽 GLES Debug 消息（PERFORMANCE/OTHER 过滤 + GL_DEBUG_OUTPUT 关闭）"
    );
}

pub fn with_egl_dispatch<F, R>(f: F) -> R
where
    F: FnOnce(&crate::egl_sys::dispatch::EglDispatch) -> R,
{
    crate::init::ensure_backend_initialized(); // 首调惰性初始化（init_egl 内部不走 with_*，无递归）
    static FIRST_EGL_CALL: AtomicBool = AtomicBool::new(true);
    if FIRST_EGL_CALL.swap(false, Ordering::Relaxed) {
        log::info!("[FluorateGL] === 首次 EGL 调用 ===");
    }

    let dispatch = EGL_DISPATCH.get().unwrap_or_else(|| {
        static STUB: OnceLock<crate::egl_sys::dispatch::EglDispatch> = OnceLock::new();
        STUB.get_or_init(crate::egl_sys::dispatch::EglDispatch::all_stub)
    });
    f(dispatch)
}

/// 返回已加载的 GLES dispatch 引用（若 EGL/GLES 库加载失败则返回 None）。
/// 用于离线编译测试等需要直接访问 GLES 的场景。
pub fn gles_dispatch() -> Option<&'static dispatch::GlesDispatch> {
    GLES_DISPATCH.get()
}

/// 返回已加载的 EGL dispatch 引用（若 EGL 库加载失败则返回 None）。
/// 用于离线编译测试等需要直接访问 EGL 的场景。
pub fn egl_dispatch() -> Option<&'static crate::egl_sys::dispatch::EglDispatch> {
    EGL_DISPATCH.get()
}

/// EGL 后端是否真实可用（EGL_DISPATCH 未设置 = stub 兜底模式）。
///
/// exports 层在 stub 模式下应拒绝返回伪指针的创建/获取类调用
/// （eglGetDisplay / eglCreateContext / eglCreate*Surface 等），
/// 直接返回 null 而非让宿主持有垃圾指针（P1-A 双层兜底）。
pub fn egl_backend_ready() -> bool {
    EGL_DISPATCH.get().is_some()
}

/// GLES 后端是否真实可用（GLES_DISPATCH 未设置 = 纯 stub 场景）。
///
/// 供 exports 层判断：GLES_DISPATCH 已设置说明宿主已绑定 GL 上下文，
/// 此时可以安全地补做能力查询；未设置（纯 stub / 离线测试）则无法查询，
/// 相关调用方应走兜底路径。
pub fn gles_dispatch_ready() -> bool {
    GLES_DISPATCH.get().is_some()
}

/// GLES 能力表是否已查询并定型（GLES_CAPABILITIES 已 set）。
///
/// 能力查询由首次 with_gles_dispatch 调用触发；glGetString / glGetStringi
/// 等路径不经 with_gles_dispatch，可能早于能力查询到达，调用方
/// （如 exports::build_fake_extensions）据此判断是否需要补查。
pub fn caps_queried() -> bool {
    GLES_CAPABILITIES.get().is_some()
}

/// 在首次 GL 调用时（EGL 上下文已创建）查询真实 GLES 版本与扩展，构建能力表。
///
/// 拦截层（drawing.rs / multi_draw.rs）基于此表决定原生转发/模拟/跳过。
/// 必须在 EGL 上下文已创建后调用，否则 glGetString 返回 null。
/// 公开为 `query_capabilities_now`：exports 层（build_fake_extensions）在
/// caps 未就绪且 GLES_DISPATCH 已设置时补调，避免扩展表构建拿到兜底 caps
/// 而错误剔除能力（S2 时序修复）。GLES_CAPABILITIES 为 OnceLock，
/// 重复调用天然幂等（首次 set 后不再覆盖）。
pub fn query_capabilities_now() {
    if let Some(dispatch) = GLES_DISPATCH.get() {
        let caps = capabilities::GlesCapabilities::query(dispatch);
        let _ = GLES_CAPABILITIES.set(caps);
    }
}

/// 返回 GLES 能力表引用。
///
/// 若尚未初始化（首次 GL 调用前，或 GLES 库加载失败），返回全 false 的兜底表。
/// 拦截层应优先用此表判断扩展支持，`is_stub` 作为函数指针层面的兜底。
pub fn capabilities() -> &'static capabilities::GlesCapabilities {
    GLES_CAPABILITIES.get().unwrap_or(&FALLBACK_CAPS)
}

/// 兜底能力表，GLES 库加载失败时使用
/// multi_draw / indirect_draw 恒 true：GLES 3.1 core 特性，项目前提
static FALLBACK_CAPS: capabilities::GlesCapabilities = capabilities::GlesCapabilities {
    version: capabilities::GlesVersion(0),
    draw_elements_base_vertex: false,
    base_instance: false,
    multi_draw_elements_base_vertex: false,
    multi_draw_indirect: false,
    multi_draw: true,
    indirect_draw: true,
    indirect_count: false,
    texture_query_lod: false,
};
