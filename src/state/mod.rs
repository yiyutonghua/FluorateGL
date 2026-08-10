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
    /// program → 是否需要在 link 时生成默认 FS（对齐 MG program.cpp
    /// ShouldGenerateFSState：0=Unknown，1=Never，2=Maybe）。
    /// attach FS → Never；仅 attach VS（且当前非 Never）→ Maybe；
    /// link 时 Maybe → 生成/复用默认 FS 并 attach（无 FS 的 program 在
    /// GLES 上 link 必败，桌面 GL 同，MG 以默认 FS 兜底）。
    pub program_should_generate_fs: FxHashMap<u32, u8>,
    /// program → 待应用的 glBindFragDataLocation 绑定（[(color, name)]）。
    /// glLinkProgram 消费后清空（对齐 MG frag_data_changed 一次性语义）。
    pub program_frag_data_bindings: FxHashMap<u32, Vec<(u32, String)>>,
    /// program → 最近 attach 的 fragment shader 桌面 id（frag_data layout
    /// 注入定位用；MG 用全局单例 shaderInfo 假设"最后 glShaderSource 的
    /// shader 即当前 FS"，per-program 跟踪更精确且线程安全）。
    pub program_fs_shader: FxHashMap<u32, u32>,
    /// GLES ES 版本号（如 320）→ 内部生成的默认 FS 的 GLES shader id（缓存复用）。
    pub default_fs_cache: FxHashMap<u32, u32>,

    pub bound_buffer: u32,
    pub bound_vertex_array: u32,
    pub bound_program: u32,
    pub bound_texture: u32,
    pub bound_framebuffer: u32,
    pub bound_renderbuffer: u32,

    /// 各 target 当前绑定的 desktop buffer ID（用于查询 target → buffer；
    /// 对应 MG buffer.cpp 的 set_bound_buffer_by_target / find_bound_buffer_by_target）
    pub bound_buffers_by_target: FxHashMap<u32, u32>,
    /// 每个 VAO 的 ELEMENT_ARRAY_BUFFER 绑定（桌面 GL 语义：IBO 绑定是 VAO state；
    /// 对应 MG buffer.cpp 的 element_array_buffer_per_vao）。glBindVertexArray 时应
    /// 用它恢复 bound_buffers_by_target 中的 ELEMENT_ARRAY_BUFFER 记录（跨域协调点，
    /// vertex_array.rs 域处理）。
    pub element_array_buffer_per_vao: FxHashMap<u32, u32>,
    /// 桌面 buffer ID → 最近一次 glBufferData/glBufferStorage 的 size（对应 MG
    /// buffer.cpp 的 g_buffer_datasize / set_buffer_data_size）。供 getter 查询
    /// GL_BUFFER_SIZE 等使用（跨域协调点，getter.rs 域对接）。
    pub buffer_sizes: FxHashMap<u32, usize>,
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
            program_should_generate_fs: FxHashMap::default(),
            program_frag_data_bindings: FxHashMap::default(),
            program_fs_shader: FxHashMap::default(),
            default_fs_cache: FxHashMap::default(),

            bound_buffer: 0,
            bound_vertex_array: 0,
            bound_program: 0,
            bound_texture: 0,
            bound_framebuffer: 0,
            bound_renderbuffer: 0,

            bound_buffers_by_target: FxHashMap::default(),
            element_array_buffer_per_vao: FxHashMap::default(),
            buffer_sizes: FxHashMap::default(),
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
