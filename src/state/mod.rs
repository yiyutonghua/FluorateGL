pub mod id_map;

use id_map::IdMap;
use std::cell::RefCell;
use std::collections::HashMap;

pub struct State {
    pub buffers: IdMap,
    pub vertex_arrays: IdMap,
    pub shaders: IdMap,
    pub shader_types: HashMap<u32, u32>,
    pub shader_sources: HashMap<u32, String>,
    pub shader_original_sources: HashMap<u32, String>,
    pub shader_translated_sources: HashMap<u32, String>,
    pub programs: IdMap,
    pub textures: IdMap,
    pub framebuffers: IdMap,
    pub renderbuffers: IdMap,
    pub queries: IdMap,

    pub bound_buffer: u32,
    pub bound_vertex_array: u32,
    pub bound_program: u32,
    pub bound_texture: u32,
    pub bound_framebuffer: u32,
    pub bound_renderbuffer: u32,
}

impl State {
    pub fn new() -> Self {
        Self {
            buffers: IdMap::new(),
            vertex_arrays: IdMap::new(),
            shaders: IdMap::new(),
            shader_types: HashMap::new(),
            shader_sources: HashMap::new(),
            shader_original_sources: HashMap::new(),
            shader_translated_sources: HashMap::new(),
            programs: IdMap::new(),
            textures: IdMap::new(),
            framebuffers: IdMap::new(),
            renderbuffers: IdMap::new(),
            queries: IdMap::new(),

            bound_buffer: 0,
            bound_vertex_array: 0,
            bound_program: 0,
            bound_texture: 0,
            bound_framebuffer: 0,
            bound_renderbuffer: 0,
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
    static FIRST_ACCESS: std::cell::Cell<bool> = std::cell::Cell::new(true);
}

pub fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut State) -> R,
{
    // 首次在该线程上访问 State 时，记录线程 ID 用于诊断跨线程问题
    FIRST_ACCESS.with(|first| {
        if first.replace(false) {
            log::info!(
                "[FluorateGL] State initialized on thread {:?} (tid={})",
                std::thread::current().name(),
                thread_id_u64()
            );
        }
    });
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// 获取当前线程 ID（用于诊断日志）
pub fn thread_id_u64() -> u64 {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::gettid() as u64
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}
