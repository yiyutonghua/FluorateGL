//! 线程局部 GL 状态：ID 映射与缓存
//!
//! 桌面 GL 与底层 GLES 的对象 ID 是两套独立命名空间（桌面 ID 由本库全局唯一
//! 单调分配，跨线程 atomic；GLES ID 由驱动分配）。[`IdMap`] 维护双向映射。
//!
//! [`State`] 是 `thread_local` 的 `RefCell`，因为 GL 上下文是线程绑定的。
//! 访问入口：
//! - [`with_state`]：可变借用（`borrow_mut`），用于写操作
//! - [`with_state_ref`]：不可变借用（`borrow`），用于只读查询，开销略低
//!
//! 额外维护一个缓存以减少热路径开销：
//! - `uniform_location_cache`：按 (program, name) 缓存 uniform location

pub mod id_map;

use id_map::IdMap;
use rustc_hash::FxHashMap;
use std::cell::RefCell;

/// 持久映射 buffer 的 CPU 端 shadow memory
///
/// GLES 3.1 不支持 GL_MAP_PERSISTENT_BIT（映射期间 buffer 仍可被 GPU 使用），
/// 用 CPU 端 shadow memory 模拟：glMapBufferRange 返回 shadow_ptr，宿主写入
/// shadow memory，draw 前 glBufferSubData 同步到 GLES buffer。
///
/// 生命周期：glBufferStorage(带 PERSISTENT) 创建 → glMapBufferRange 返回 shadow_ptr →
/// glFlushMappedBufferRange 标记脏区域 → draw 前同步 → glDeleteBuffers 释放。
pub struct PersistentMapping {
    /// CPU 端分配的 shadow memory 起始指针
    pub shadow_ptr: *mut u8,
    /// shadow memory 总大小（字节数）
    pub shadow_size: usize,
    /// 对应的 GLES buffer ID（用于 glBufferSubData 同步）
    pub gles_buffer_id: u32,
    /// 未同步的脏数据起始偏移
    pub dirty_offset: usize,
    /// 未同步的脏数据长度
    pub dirty_length: usize,
    /// 该 buffer 当前绑定的 GL target（glBindBuffer / glBindBufferBase /
    /// glBindBufferRange 更新；sync 时按此 target 定位 glBufferSubData 上传目标，
    /// 使 GL_UNIFORM_BUFFER 等所有走 shadow 路径的 target 都能被同步）
    pub bound_target: u32,
}

// shadow_ptr 由 libc::malloc 分配，跨线程不共享（thread_local State），Send/Sync 安全
unsafe impl Send for PersistentMapping {}

pub struct State {
    pub buffers: IdMap,
    pub vertex_arrays: IdMap,
    pub shaders: IdMap,
    pub shader_types: FxHashMap<u32, u32>,
    pub shader_sources: FxHashMap<u32, String>,
    pub shader_original_sources: FxHashMap<u32, String>,
    pub programs: IdMap,
    pub textures: IdMap,
    pub framebuffers: IdMap,
    pub renderbuffers: IdMap,
    pub queries: IdMap,
    /// uniform location 缓存（key = (desktop_program_id, uniform_name)）
    pub uniform_location_cache: FxHashMap<(u32, String), i32>,

    pub bound_buffer: u32,
    pub bound_vertex_array: u32,
    pub bound_program: u32,
    pub bound_texture: u32,
    pub bound_framebuffer: u32,
    pub bound_renderbuffer: u32,

    /// 持久映射 buffer 的 shadow memory（key = desktop buffer ID）
    pub persistent_buffers: FxHashMap<u32, PersistentMapping>,
    /// 各 target 当前绑定的 desktop buffer ID（用于查询 target → buffer）
    pub bound_buffers_by_target: FxHashMap<u32, u32>,
}

impl State {
    pub fn new() -> Self {
        Self {
            buffers: IdMap::new(),
            vertex_arrays: IdMap::new(),
            shaders: IdMap::new(),
            shader_types: FxHashMap::default(),
            shader_sources: FxHashMap::default(),
            shader_original_sources: FxHashMap::default(),
            programs: IdMap::new(),
            textures: IdMap::new(),
            framebuffers: IdMap::new(),
            renderbuffers: IdMap::new(),
            queries: IdMap::new(),
            uniform_location_cache: FxHashMap::default(),

            bound_buffer: 0,
            bound_vertex_array: 0,
            bound_program: 0,
            bound_texture: 0,
            bound_framebuffer: 0,
            bound_renderbuffer: 0,

            persistent_buffers: FxHashMap::default(),
            bound_buffers_by_target: FxHashMap::default(),
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
    static FIRST_ACCESS: std::cell::Cell<bool> = std::cell::Cell::new(true);
    // 缓存线程 ID，避免每次日志都调用 libc::gettid() 系统调用。
    // 线程 ID 在同一线程内不变，首次计算后缓存。
    static CACHED_TID: std::cell::OnceCell<u64> = std::cell::OnceCell::new();
}

/// 写访问 State（borrow_mut）
pub fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut State) -> R,
{
    // 首次访问日志用 log_enabled! 包裹，日志关闭时跳过 FIRST_ACCESS 的 thread_local 访问
    if log::log_enabled!(log::Level::Info) {
        FIRST_ACCESS.with(|first| {
            if first.replace(false) {
                log::info!(
                    "[FluorateGL] State initialized on thread {:?} (tid={})",
                    std::thread::current().name(),
                    thread_id_u64()
                );
            }
        });
    }
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// 只读访问 State（borrow），开销略低于 with_state 的 borrow_mut
pub fn with_state_ref<F, R>(f: F) -> R
where
    F: FnOnce(&State) -> R,
{
    STATE.with(|s| f(&s.borrow()))
}

/// 获取当前线程 ID（用于诊断日志）
///
/// 使用 thread_local OnceCell 缓存，同一线程仅首次调用 libc::gettid()，
/// 后续直接返回缓存值，消除热路径上的系统调用开销。
///
/// 注意：Android 的 target_os 是 "android" 而非 "linux"，
/// 之前只匹配 linux 导致 Android 上 tid 始终返回 0，无法区分异步线程。
pub fn thread_id_u64() -> u64 {
    CACHED_TID.with(|cell| *cell.get_or_init(|| compute_tid()))
}

fn compute_tid() -> u64 {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe {
        libc::gettid() as u64
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        0
    }
}
