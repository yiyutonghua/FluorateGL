use crate::backend;
use crate::state;
use std::sync::atomic::{AtomicBool, Ordering};

// glMapBufferRange access bits（桌面 GL 与 GLES 共享的低 16 位语义）
// GLES 3.1 仅支持 READ/WRITE/INVALIDATE_RANGE/INVALIDATE_BUFFER/FLUSH_EXPLICIT/UNSYNCHRONIZED，
// 不支持 PERSISTENT(0x0040)/COHERENT(0x0080)（桌面 GL 4.4 / GL_ARB_buffer_storage 引入）。
const GL_MAP_READ_BIT: u32 = 0x0001;
const GL_MAP_WRITE_BIT: u32 = 0x0002;
const GL_MAP_UNSYNCHRONIZED_BIT: u32 = 0x0020;
const GL_MAP_PERSISTENT_BIT: u32 = 0x0040;
const GL_MAP_COHERENT_BIT: u32 = 0x0080;

/// 将桌面 GL 的 glMapBufferRange access flags 翻译为 GLES 3.1 支持的位。
///
/// GLES 3.1 不支持 PERSISTENT/COHERENT 位，需剥离：
/// - PERSISTENT（映射期间 buffer 仍可被 GPU 使用）：无法完美模拟，剥离后配合
///   UNSYNCHRONIZED 可近似 Sodium 的流式上传场景（每帧写入新区域 + fence 同步）。
/// - COHERENT（GPU/CPU 访问自动可见）：用 UNSYNCHRONIZED 替代语义（都是不自动同步），
///   Sodium 自管理同步，剥离后功能正确。
///
/// 剥离后若没有任何有效的读写位，补 GL_MAP_WRITE_BIT 避免 GLES 返回 NULL。
fn translate_map_access(access: u32) -> u32 {
    let mut out = access & !GL_MAP_PERSISTENT_BIT;
    let had_coherent = out & GL_MAP_COHERENT_BIT != 0;
    out &= !GL_MAP_COHERENT_BIT;
    if had_coherent {
        out |= GL_MAP_UNSYNCHRONIZED_BIT;
    }
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

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenBuffers(n: i32, buffers: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let mut gles_id = 0u32;
            (dispatch.gen_buffers)(1, &mut gles_id);

            let desktop_id = state::with_state(|s| s.buffers.alloc(gles_id));
            log::debug!(
                "[FluorateGL] glGenBuffers: GLES {} -> desktop {} (tid={})",
                gles_id,
                desktop_id,
                state::thread_id_u64()
            );
            *buffers.offset(i) = desktop_id;
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteBuffers(n: i32, buffers: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let desktop_id = *buffers.offset(i);
            // 释放持久映射的 shadow memory（若存在）
            state::with_state(|s| {
                if let Some(pm) = s.persistent_buffers.remove(&desktop_id) {
                    libc::free(pm.shadow_ptr as *mut libc::c_void);
                    log::debug!(
                        "[FluorateGL] glDeleteBuffers: freed shadow memory (desktop={}, size={})",
                        desktop_id, pm.shadow_size
                    );
                }
            });
            if let Some(gles_id) = state::with_state(|s| s.buffers.delete(desktop_id)) {
                log::debug!(
                    "[FluorateGL] glDeleteBuffers: desktop {} -> GLES {} (deleted, tid={})",
                    desktop_id,
                    gles_id,
                    state::thread_id_u64()
                );
                (dispatch.delete_buffers)(1, &gles_id);
            } else {
                log::debug!(
                    "[FluorateGL] glDeleteBuffers: desktop {} NOT FOUND in IdMap, ignored",
                    desktop_id
                );
            }
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindBuffer(target: u32, buffer: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                warn_buffer_id_miss("glBindBuffer", target, buffer);
                0
            })
            })
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

        // 记录 target → desktop buffer ID 映射，供持久映射模拟查询
        state::with_state(|s| {
            s.bound_buffers_by_target.insert(target, buffer);
            if target == 0x8892 || target == 0x8893 {
                s.bound_buffer = buffer;
            }
        });
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.buffer_data)(target, size, data, usage);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferSubData(
    target: u32,
    offset: isize,
    size: isize,
    data: *const std::ffi::c_void,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.buffer_sub_data)(target, offset, size, data);
    });
}

fn is_stub(dispatch: &backend::dispatch::GlesDispatch, f: *const ()) -> bool {
    f == dispatch.stub as *const ()
}

/// GL_BUFFER_STORAGE_FLAGS 查询的 bit（桌面 GL 4.4 / GL_ARB_buffer_storage）
const GL_MAP_PERSISTENT_BIT_STORAGE: u32 = 0x0040;

/// 同步持久映射 buffer 的 shadow memory 到 GLES buffer（若该 buffer 是持久映射的）。
///
/// 在 draw call 前调用，确保 GLES buffer 包含 shadow memory 的最新数据。
/// 仅同步脏区域（dirty_offset..dirty_offset+dirty_length），用 glBufferSubData 上传。
pub(crate) fn sync_persistent_buffer_if_needed(target: u32) {
    let desktop_id = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    let Some(desktop_id) = desktop_id else { return };

    let pm_info = state::with_state(|s| {
        s.persistent_buffers.get_mut(&desktop_id).map(|pm| {
            let (off, len) = if pm.dirty_length == 0 {
                (0usize, 0usize)
            } else {
                (pm.dirty_offset, pm.dirty_length)
            };
            pm.dirty_offset = 0;
            pm.dirty_length = 0;
            (pm.shadow_ptr, pm.shadow_size, off, len, pm.gles_buffer_id)
        })
    });
    let Some((shadow_ptr, _shadow_size, off, len, _gles_id)) = pm_info else {
        return;
    };

    if len == 0 {
        return;
    }

    backend::with_gles_dispatch(|dispatch| unsafe {
        let ptr = shadow_ptr.add(off) as *const std::ffi::c_void;
        (dispatch.buffer_sub_data)(target, off as isize, len as isize, ptr);
        log::debug!(
            "[FluorateGL] sync_persistent_buffer: target=0x{:04X} desktop={} offset={} len={}",
            target, desktop_id, off, len
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBufferStorage(
    target: u32,
    size: isize,
    data: *const std::ffi::c_void,
    flags: u32,
) {
    // 带 PERSISTENT 位时，在 CPU 端分配 shadow memory 模拟持久映射
    let is_persistent = flags & GL_MAP_PERSISTENT_BIT_STORAGE != 0 && size > 0;

    if is_persistent {
        // 查 target 绑定的 desktop buffer ID 和 GLES buffer ID
        let desktop_id = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
        let gles_id = state::with_state_ref(|s| {
            desktop_id.and_then(|id| s.buffers.get_gles(id))
        });

        if let (Some(desktop_id), Some(gles_id)) = (desktop_id, gles_id) {
            let alloc_size = size as usize;
            let shadow_ptr = unsafe { libc::malloc(alloc_size) as *mut u8 };
            if !shadow_ptr.is_null() {
                // 初始数据拷贝
                if !data.is_null() {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            data as *const u8,
                            shadow_ptr,
                            alloc_size,
                        );
                    }
                }
                state::with_state(|s| {
                    s.persistent_buffers.insert(
                        desktop_id,
                        state::PersistentMapping {
                            shadow_ptr,
                            shadow_size: alloc_size,
                            gles_buffer_id: gles_id,
                            dirty_offset: 0,
                            dirty_length: alloc_size, // 初始全量同步
                        },
                    );
                });
                log::debug!(
                    "[FluorateGL] glBufferStorage: persistent shadow memory allocated (target=0x{:04X} desktop={} gles={} size={})",
                    target, desktop_id, gles_id, alloc_size
                );
            }
        }
    }

    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.buffer_storage as *const ()) {
            // 驱动不支持 GL_EXT_buffer_storage，降级为 glBufferData（GL_DYNAMIC_DRAW）
            (dispatch.buffer_data)(target, size, data, 0x88E8);
        } else {
            // 原生路径：驱动支持 GL_EXT_buffer_storage，原样传 flags
            (dispatch.buffer_storage)(target, size, data, flags);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glMapBuffer(target: u32, access: u32) -> *mut std::ffi::c_void {
    backend::with_gles_dispatch(|dispatch| unsafe {
        // GLES 不提供 glMapBuffer（仅 glMapBufferRange），用 glMapBufferRange 模拟。
        // 若 map_buffer_range 也是 stub（驱动不支持），返回 null 避免后续 UB。
        if is_stub(dispatch, dispatch.map_buffer_range as *const ()) {
            warn_map_buffer_unavailable();
            return std::ptr::null_mut();
        }

        let mut size = 0i32;
        (dispatch.get_buffer_parameter_iv)(target, 0x8764, &mut size); // GL_BUFFER_SIZE

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

        (dispatch.map_buffer_range)(target, 0, size as isize, range_access)
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 剥离 GLES 不支持的 PERSISTENT/COHERENT 位，否则 GLES 返回 NULL
        let gles_access = translate_map_access(access);
        (dispatch.map_buffer_range)(target, offset, length, gles_access)
    })
}
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUnmapBuffer(target: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.unmap_buffer)(target) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFlushMappedBufferRange(target: u32, offset: isize, length: isize) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.flush_mapped_buffer_range)(target, offset, length);
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
        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                warn_buffer_id_miss("glBindBufferBase", target, buffer);
                0
            })
            })
        };

        (dispatch.bind_buffer_base)(target, index, gles_id);
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
        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                warn_buffer_id_miss("glBindBufferRange", target, buffer);
                0
            })
            })
        };

        (dispatch.bind_buffer_range)(target, index, gles_id, offset, size);
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
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_buffer_parameter_iv)(target, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetBufferPointerv(target: u32, pname: u32, params: *mut *mut std::ffi::c_void) {
    if params.is_null() {
        return;
    }
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

    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.buffers.get_gles(buffer).unwrap_or(0));
        (dispatch.is_buffer)(gles_id)
    })
}

// glTexBuffer 将 buffer 绑定到纹理，buffer ID 需要从 desktop 翻译为 GLES。

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTexBuffer(target: u32, internalformat: u32, buffer: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        if is_stub(dispatch, dispatch.tex_buffer as *const ()) {
            warn_tex_buffer_stub("glTexBuffer");
            return;
        }

        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                    warn_tex_buffer_id_miss("glTexBuffer", target, buffer);
                    0
                })
            })
        };

        log::debug!(
            "[FluorateGL] glTexBuffer(target=0x{:04X}, fmt=0x{:04X}) desktop {} -> GLES {} (tid={})",
            target,
            internalformat,
            buffer,
            gles_id,
            state::thread_id_u64()
        );

        (dispatch.tex_buffer)(target, internalformat, gles_id);
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

        let gles_id = if buffer == 0 {
            0
        } else {
            state::with_state(|s| {
                s.buffers.get_gles(buffer).unwrap_or_else(|| {
                    warn_tex_buffer_id_miss("glTexBufferRange", target, buffer);
                    0
                })
            })
        };

        log::debug!(
            "[FluorateGL] glTexBufferRange(target=0x{:04X}, fmt=0x{:04X}) desktop {} -> GLES {} (tid={})",
            target,
            internalformat,
            buffer,
            gles_id,
            state::thread_id_u64()
        );

        (dispatch.tex_buffer_range)(target, internalformat, gles_id, offset, size);
    });
}
