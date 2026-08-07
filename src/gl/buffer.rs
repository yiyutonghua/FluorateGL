use crate::backend;
use crate::state;
use std::sync::atomic::{AtomicBool, Ordering};

// glMapBufferRange access bits（桌面 GL 与 GLES 共享的低 16 位语义）
// GLES 3.1 仅支持 READ/WRITE/INVALIDATE_RANGE/INVALIDATE_BUFFER/FLUSH_EXPLICIT/UNSYNCHRONIZED，
// 不支持 PERSISTENT(0x0040)/COHERENT(0x0080)（桌面 GL 4.4 / GL_ARB_buffer_storage 引入）。
const GL_MAP_READ_BIT: u32 = 0x0001;
const GL_MAP_WRITE_BIT: u32 = 0x0002;
const GL_MAP_PERSISTENT_BIT: u32 = 0x0040;
const GL_MAP_COHERENT_BIT: u32 = 0x0080;

/// GL_PARAMETER_BUFFER（GL 4.6 引入，glMultiDraw*IndirectCount 的 count 来源）
/// GLES 不识别该 target，下传会触发 GL_INVALID_ENUM，仅在 state 中记录绑定。
const GL_PARAMETER_BUFFER: u32 = 0x80EE;
/// GL_COPY_READ_BUFFER：用于临时绑定 count buffer 读取数据（合法的通用 target）
const GL_COPY_READ_BUFFER: u32 = 0x8F36;

/// 将桌面 GL 的 glMapBufferRange access flags 翻译为 GLES 3.1 支持的位。
///
/// GLES 3.1 不支持 PERSISTENT/COHERENT 位，需剥离：
/// - PERSISTENT（映射期间 buffer 仍可被 GPU 使用）：无法完美模拟，剥离后配合
///   shadow 路径（持久映射 buffer 走 shadow_ptr）保证数据同步。
/// - COHERENT（GPU/CPU 访问自动可见）：GLES 无对应语义，直接剥离，保留 GLES
///   默认的显式 flush 语义（映射后 flush 数据才可见）。
///   注意：不能转成 UNSYNCHRONIZED——两者语义不同（UNSYNCHRONIZED 是跳过
///   映射前的同步保护，可能与 in-flight draw 产生竞态），故直接丢弃该位。
///
/// 剥离后若没有任何有效的读写位，补 GL_MAP_WRITE_BIT 避免 GLES 返回 NULL。
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
                        desktop_id,
                        pm.shadow_size
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
    // GL_PARAMETER_BUFFER 是 GL 4.6 引入的 target（glMultiDraw*IndirectCount 的 count 来源），
    // GLES 不识别该 target，下传会触发 GL_INVALID_ENUM。仅记录 state 用于 CPU 端模拟。
    if target == GL_PARAMETER_BUFFER {
        state::with_state(|s| {
            s.bound_buffers_by_target.insert(target, buffer);
        });
        log::debug!(
            "[FluorateGL] glBindBuffer(GL_PARAMETER_BUFFER): desktop {} recorded (not forwarded, tid={})",
            buffer,
            state::thread_id_u64()
        );
        return;
    }

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
            // 持久映射 buffer 记录绑定 target（sync 时按 bound_target 定位上传目标）
            if let Some(pm) = s.persistent_buffers.get_mut(&buffer) {
                pm.bound_target = target;
            } else if buffer == 0 {
                // 解绑：清空该 target 上持久映射条目的绑定标记
                for pm in s.persistent_buffers.values_mut() {
                    if pm.bound_target == target {
                        pm.bound_target = 0;
                    }
                }
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
    // GL_PARAMETER_BUFFER 用 shadow memory 管理，不下传 GLES
    if target == GL_PARAMETER_BUFFER {
        let desktop_id = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
        if let Some(desktop_id) = desktop_id {
            let alloc_size = if size > 0 { size as usize } else { 0 };
            state::with_state(|s| {
                // 释放旧 shadow（重新分配）
                if let Some(old) = s.persistent_buffers.remove(&desktop_id) {
                    unsafe { libc::free(old.shadow_ptr as *mut libc::c_void) };
                }
                if alloc_size > 0 {
                    let shadow_ptr = unsafe { libc::malloc(alloc_size) as *mut u8 };
                    if !shadow_ptr.is_null() {
                        if !data.is_null() {
                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    data as *const u8,
                                    shadow_ptr,
                                    alloc_size,
                                );
                            }
                        }
                        s.persistent_buffers.insert(
                            desktop_id,
                            state::PersistentMapping {
                                shadow_ptr,
                                shadow_size: alloc_size,
                                gles_buffer_id: 0,
                                dirty_offset: 0,
                                dirty_length: 0,
                                // PARAMETER_BUFFER 不下传 GLES，不参与 sync
                                bound_target: 0,
                            },
                        );
                    }
                }
            });
            log::debug!(
                "[FluorateGL] glBufferData(GL_PARAMETER_BUFFER): shadow size={}",
                alloc_size
            );
        }
        return;
    }

    // 仅持久映射 buffer（已通过 glBufferStorage(PERSISTENT) 创建 shadow）需要重新分配 shadow：
    // 大小可能变化，先释放旧 shadow 再分配新的。普通 buffer 的 glBufferData 只下传 GLES，
    // 不创建 shadow——否则 glMapBufferRange 会误走 shadow 路径返回 shadow_ptr，
    // 而 dirty_length=0（本函数所设）导致 draw 前 sync 空转，宿主写入的数据丢失在 shadow 中，
    // GLES buffer 只有初始数据，造成红屏/UI 消失。
    let shadow_realloc = state::with_state_ref(|s| {
        let desktop_id = s.bound_buffers_by_target.get(&target).copied()?;
        // 仅当该 buffer 已是持久映射的才需要重新分配 shadow
        if s.persistent_buffers.contains_key(&desktop_id) {
            Some(desktop_id)
        } else {
            None
        }
    });
    if let Some(desktop_id) = shadow_realloc {
        let gles_id = state::with_state_ref(|s| s.buffers.get_gles(desktop_id));
        if let Some(gles_id) = gles_id {
            let alloc_size = if size > 0 { size as usize } else { 0 };
            state::with_state(|s| {
                // 释放旧 shadow（大小可能变化）
                if let Some(old) = s.persistent_buffers.remove(&desktop_id) {
                    unsafe { libc::free(old.shadow_ptr as *mut libc::c_void) };
                }
                if alloc_size > 0 {
                    let shadow_ptr = unsafe { libc::malloc(alloc_size) as *mut u8 };
                    if !shadow_ptr.is_null() {
                        if !data.is_null() {
                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    data as *const u8,
                                    shadow_ptr,
                                    alloc_size,
                                );
                            }
                        }
                        s.persistent_buffers.insert(
                            desktop_id,
                            state::PersistentMapping {
                                shadow_ptr,
                                shadow_size: alloc_size,
                                gles_buffer_id: gles_id,
                                dirty_offset: 0,
                                // glBufferData 已下传 GLES，无需再 sync
                                dirty_length: 0,
                                // 当前绑定 target（glBindBuffer 已先于 BufferData 发生）
                                bound_target: target,
                            },
                        );
                    }
                }
            });
            log::debug!(
                "[FluorateGL] glBufferData(0x{:04X}): persistent shadow reallocated size={}",
                target,
                alloc_size
            );
        }
    }

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
    // GL_PARAMETER_BUFFER 写入 shadow memory，不下传 GLES（非法 target）
    if target == GL_PARAMETER_BUFFER {
        state::with_state(|s| {
            let desktop_id = match s.bound_buffers_by_target.get(&target).copied() {
                Some(id) => id,
                None => return,
            };
            if let Some(pm) = s.persistent_buffers.get_mut(&desktop_id) {
                let off = if offset > 0 { offset as usize } else { 0 };
                let len = if size > 0 { size as usize } else { 0 };
                if off + len <= pm.shadow_size && !data.is_null() {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            data as *const u8,
                            pm.shadow_ptr.add(off),
                            len,
                        );
                    }
                }
            }
        });
        return;
    }

    // 持久映射 buffer：同时更新 shadow memory（保持 shadow 与 GLES buffer 一致），
    // 并下传 GLES。避免宿主混合使用 map 和 sub_data 写入时数据不一致。
    state::with_state(|s| {
        let Some(desktop_id) = s.bound_buffers_by_target.get(&target).copied() else {
            return;
        };
        let Some(pm) = s.persistent_buffers.get_mut(&desktop_id) else {
            return;
        };
        let off = if offset > 0 { offset as usize } else { 0 };
        let len = if size > 0 { size as usize } else { 0 };
        if off + len <= pm.shadow_size && !data.is_null() {
            unsafe {
                std::ptr::copy_nonoverlapping(data as *const u8, pm.shadow_ptr.add(off), len);
            }
        }
    });

    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.buffer_sub_data)(target, offset, size, data);
    });
}

fn is_stub(dispatch: &backend::dispatch::GlesDispatch, f: *const ()) -> bool {
    f == dispatch.stub as *const ()
}

/// GL_BUFFER_STORAGE_FLAGS 查询的 bit（桌面 GL 4.4 / GL_ARB_buffer_storage）
const GL_MAP_PERSISTENT_BIT_STORAGE: u32 = 0x0040;

/// 同步所有持久映射 buffer 的 shadow memory 到 GLES buffer（若存在脏区域）。
///
/// 在 draw call 前调用，确保 GLES buffer 包含 shadow memory 的最新数据。
/// **全量遍历** `persistent_buffers` 中 `dirty_length > 0` 的条目，按各自记录的
/// `bound_target` 用 glBufferSubData 上传——GL_UNIFORM_BUFFER / TRANSFORM_FEEDBACK_BUFFER
/// / SHADER_STORAGE_BUFFER 等所有走 shadow 路径的 target 都被覆盖，不再依赖调用方
/// 传入的 target（修复：UBO shadow 脏区永不消费导致矩阵恒 0 的黑屏根因）。
///
/// `target` 参数保留以兼容既有 28 处调用点（GL_ARRAY_BUFFER / GL_ELEMENT_ARRAY_BUFFER /
/// GL_DRAW_INDIRECT_BUFFER），但同步范围以全量遍历为准；该参数仅用于命中时的日志过滤。
///
/// 借用约束：先在 with_state 中收集待同步列表（同时消费清零 dirty），再在
/// with_gles_dispatch 中执行 glBufferSubData，避免 RefCell 借用冲突。
///
/// 同步目标定位：shadow 条目记录 `bound_target`（glBindBuffer / glBindBufferBase /
/// glBindBufferRange 更新）。若该 target 当前绑定的 GLES buffer 与 shadow 的
/// gles_buffer_id 不一致（宿主 flush 后改绑的异常路径），临时改绑原 target 上传后
/// 恢复原绑定（GLES 的通用绑定点与 BindBufferBase 的索引绑定相互独立，改绑不影响
/// 索引绑定的实际使用）。
/// GL_PARAMETER_BUFFER 无对应 GLES buffer（gles_buffer_id=0），跳过同步。
pub(crate) fn sync_persistent_buffer_if_needed(target: u32) {
    // 收集所有脏区域（消费并清零 dirty，避免重复上传）
    // 元组：(desktop_id, gles_id, offset, length, shadow_ptr, bound_target)
    let pending: Vec<(u32, u32, usize, usize, *mut u8, u32)> = state::with_state(|s| {
        s.persistent_buffers
            .iter_mut()
            .filter(|(_, pm)| pm.dirty_length > 0 && pm.gles_buffer_id != 0 && pm.bound_target != 0)
            .map(|(desktop_id, pm)| {
                let (off, len) = (pm.dirty_offset, pm.dirty_length);
                pm.dirty_offset = 0;
                pm.dirty_length = 0;
                (
                    *desktop_id,
                    pm.gles_buffer_id,
                    off,
                    len,
                    pm.shadow_ptr,
                    pm.bound_target,
                )
            })
            .collect()
    });
    if pending.is_empty() {
        return;
    }

    backend::with_gles_dispatch(|dispatch| unsafe {
        for (desktop_id, gles_id, off, len, shadow_ptr, bound_target) in &pending {
            // 校验/恢复：该 target 当前绑定的 GLES buffer 是否就是 shadow 的 GLES buffer
            let current_gles = state::with_state_ref(|s| {
                s.bound_buffers_by_target
                    .get(bound_target)
                    .copied()
                    .and_then(|d| s.buffers.get_gles(d))
                    .unwrap_or(0)
            });
            let needs_restore = current_gles != *gles_id;
            if needs_restore {
                (dispatch.bind_buffer)(*bound_target, *gles_id);
            }
            let ptr = shadow_ptr.add(*off) as *const std::ffi::c_void;
            (dispatch.buffer_sub_data)(*bound_target, *off as isize, *len as isize, ptr);
            if needs_restore {
                (dispatch.bind_buffer)(*bound_target, current_gles);
            }
            log::debug!(
                "[FluorateGL] sync_persistent_buffer: target=0x{:04X} desktop={} offset={} len={} (target_arg=0x{:04X})",
                bound_target,
                desktop_id,
                off,
                len,
                target
            );
        }
    });
}

/// 从 GL_PARAMETER_BUFFER 读取实际 draw count（u32），用于模拟 glMultiDraw*IndirectCount。
///
/// 原生 GLES 不支持从 GPU buffer 读 count，需在 CPU 侧读出后循环调用对应的
/// 单次 Indirect。两级读取策略：
///
/// 1. **shadow memory**（Sodium 典型场景，count buffer 是持久映射的）：
///    直接从 `shadow_ptr + offset` 读 4 字节，零 GLES 调用、零同步开销。
///    shadow memory 是宿主写入的唯一目的地，是 CPU 可见的最新数据源。
/// 2. **glMapBufferRange 兜底**（非持久映射的 count buffer）：
///    借 GL_COPY_READ_BUFFER 临时绑定 count buffer → `glMapBufferRange(READ_BIT)`
///    读 4 字节 → `glUnmapBuffer` → 恢复原绑定。
///
/// 返回 None 表示 count buffer 未绑定 / 读取失败，调用方应跳过本次 draw。
pub(crate) fn read_parameter_buffer_u32(offset: isize) -> Option<u32> {
    if offset < 0 {
        return None;
    }

    // 路径 1: shadow memory（持久映射 buffer）
    let shadow_read = state::with_state_ref(|s| {
        let desktop_id = s
            .bound_buffers_by_target
            .get(&GL_PARAMETER_BUFFER)
            .copied()?;
        let pm = s.persistent_buffers.get(&desktop_id)?;
        let off = offset as usize;
        if off.checked_add(4)? > pm.shadow_size {
            return None;
        }
        // SAFETY: shadow_ptr 由 malloc 分配，已校验 offset+4 在 shadow_size 范围内
        let val = unsafe { std::ptr::read_unaligned(pm.shadow_ptr.add(off) as *const u32) };
        Some(val)
    });
    if let Some(v) = shadow_read {
        log::debug!(
            "[FluorateGL] read_parameter_buffer_u32: shadow read offset={} count={}",
            offset,
            v
        );
        return Some(v);
    }

    // 路径 2: glMapBufferRange 兜底
    // GL_PARAMETER_BUFFER 是非法 target，需借 GL_COPY_READ_BUFFER 临时绑定
    let desktop_id =
        state::with_state_ref(|s| s.bound_buffers_by_target.get(&GL_PARAMETER_BUFFER).copied())?;
    let gles_id = state::with_state_ref(|s| s.buffers.get_gles(desktop_id))?;

    // 保存 GL_COPY_READ_BUFFER 原绑定以便恢复（避免污染宿主状态）
    let prev_gles = state::with_state_ref(|s| {
        s.bound_buffers_by_target
            .get(&GL_COPY_READ_BUFFER)
            .copied()
            .and_then(|d| s.buffers.get_gles(d))
    })
    .unwrap_or(0);

    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.bind_buffer)(GL_COPY_READ_BUFFER, gles_id);
        let ptr = (dispatch.map_buffer_range)(GL_COPY_READ_BUFFER, offset, 4, GL_MAP_READ_BIT);
        if ptr.is_null() {
            log::warn!(
                "[FluorateGL] read_parameter_buffer_u32: map_range failed (offset={})",
                offset
            );
            (dispatch.bind_buffer)(GL_COPY_READ_BUFFER, prev_gles);
            return None;
        }
        let val = std::ptr::read_unaligned(ptr as *const u32);
        (dispatch.unmap_buffer)(GL_COPY_READ_BUFFER);
        (dispatch.bind_buffer)(GL_COPY_READ_BUFFER, prev_gles);
        log::debug!(
            "[FluorateGL] read_parameter_buffer_u32: map_range read offset={} count={}",
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
    // GL_PARAMETER_BUFFER 是非法 GLES target，必须用 shadow memory 模拟
    let is_parameter_buffer = target == GL_PARAMETER_BUFFER;
    // 带 PERSISTENT 位 或 GL_PARAMETER_BUFFER 时，在 CPU 端分配 shadow memory 模拟持久映射
    let need_shadow =
        (flags & GL_MAP_PERSISTENT_BIT_STORAGE != 0 || is_parameter_buffer) && size > 0;

    if need_shadow {
        // 查 target 绑定的 desktop buffer ID 和 GLES buffer ID
        let desktop_id = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
        let gles_id = state::with_state_ref(|s| desktop_id.and_then(|id| s.buffers.get_gles(id)));

        if let (Some(desktop_id), Some(gles_id)) = (desktop_id, gles_id) {
            let alloc_size = size as usize;
            // 释放已有 shadow（重新分配场景）
            state::with_state(|s| {
                if let Some(old) = s.persistent_buffers.remove(&desktop_id) {
                    unsafe { libc::free(old.shadow_ptr as *mut libc::c_void) };
                }
            });
            let shadow_ptr = unsafe { libc::malloc(alloc_size) as *mut u8 };
            if !shadow_ptr.is_null() {
                // 初始数据拷贝
                if !data.is_null() {
                    unsafe {
                        std::ptr::copy_nonoverlapping(data as *const u8, shadow_ptr, alloc_size);
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
                            // GL_PARAMETER_BUFFER 不需要同步到 GLES（无 GLES buffer），
                            // 持久映射的普通 buffer 初始全量同步
                            dirty_length: if is_parameter_buffer { 0 } else { alloc_size },
                            // PARAMETER_BUFFER 是非法 GLES target，不参与 sync
                            bound_target: if is_parameter_buffer { 0 } else { target },
                        },
                    );
                });
                log::debug!(
                    "[FluorateGL] glBufferStorage: shadow memory allocated (target=0x{:04X} desktop={} gles={} size={} persistent={})",
                    target,
                    desktop_id,
                    gles_id,
                    alloc_size,
                    !is_parameter_buffer
                );
            }
        }
    }

    // GL_PARAMETER_BUFFER 不下传 GLES（非法 target，数据由 shadow 管理）
    if is_parameter_buffer {
        return;
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
    // 持久映射 buffer（含 GL_PARAMETER_BUFFER）走 shadow 路径，委托给 glMapBufferRange
    let is_persistent = state::with_state_ref(|s| {
        let desktop_id = s.bound_buffers_by_target.get(&target).copied()?;
        s.persistent_buffers
            .get(&desktop_id)
            .map(|pm| pm.shadow_size as isize)
    });
    if let Some(size) = is_persistent {
        if size <= 0 {
            return std::ptr::null_mut();
        }
        // 复用 glMapBufferRange 的 shadow 路径逻辑
        return glMapBufferRange(target, 0, size, access);
    }

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
    // 持久映射 buffer（含 GL_PARAMETER_BUFFER）返回 shadow_ptr + offset，不下传 GLES。
    // shadow_ptr 是宿主写入的唯一目的地，draw 前 sync_persistent_buffer_if_needed
    // 把脏区域同步到 GLES buffer。这样 GLES buffer 不会长时间处于 mapped 状态，
    // 避免 Adreno 驱动对"mapped 状态下 draw"报 GL_INVALID_OPERATION。
    let shadow_ptr = state::with_state_ref(|s| {
        let desktop_id = s.bound_buffers_by_target.get(&target).copied()?;
        let pm = s.persistent_buffers.get(&desktop_id)?;
        let off = if offset > 0 { offset as usize } else { 0 };
        let len = if length > 0 { length as usize } else { 0 };
        if off + len > pm.shadow_size {
            return None;
        }
        // SAFETY: shadow_ptr 由 malloc 分配，已校验 off+len 在 shadow_size 范围内
        Some(unsafe { pm.shadow_ptr.add(off) as *mut std::ffi::c_void })
    });
    if let Some(ptr) = shadow_ptr {
        log::debug!(
            "[FluorateGL] glMapBufferRange(0x{:04X}): shadow path offset={} length={}",
            target,
            offset,
            length
        );
        return ptr;
    }

    // 诊断：非 shadow path 的映射，确认 Sodium 是否对普通 buffer 调用了 map
    let bound_desktop = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    log::debug!(
        "[FluorateGL] glMapBufferRange(0x{:04X}): GLES native path offset={} length={} access=0x{:04X} bound_buffer={:?}",
        target,
        offset,
        length,
        access,
        bound_desktop
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        // 剥离 GLES 不支持的 PERSISTENT/COHERENT 位，否则 GLES 返回 NULL
        let gles_access = translate_map_access(access);
        (dispatch.map_buffer_range)(target, offset, length, gles_access)
    })
}
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUnmapBuffer(target: u32) -> u8 {
    // 持久映射 buffer（含 GL_PARAMETER_BUFFER）的 shadow memory 无需 unmap。
    // 持久映射语义是"map 一次永不 unmap"，shadow_ptr 始终有效，直接返回成功。
    let is_persistent = state::with_state_ref(|s| {
        let desktop_id = s.bound_buffers_by_target.get(&target).copied()?;
        // 只要该 target 绑定的 buffer 在 persistent_buffers 表中，即为持久映射
        s.persistent_buffers.contains_key(&desktop_id).then_some(())
    });
    if is_persistent.is_some() {
        return 1;
    }
    // 诊断：非 persistent path 的 unmap，确认普通 buffer 映射生命周期
    let bound_desktop = state::with_state_ref(|s| s.bound_buffers_by_target.get(&target).copied());
    log::debug!(
        "[FluorateGL] glUnmapBuffer(0x{:04X}): GLES native path bound_buffer={:?}",
        target,
        bound_desktop
    );
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.unmap_buffer)(target) })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFlushMappedBufferRange(target: u32, offset: isize, length: isize) {
    // 持久映射 buffer（含 GL_PARAMETER_BUFFER）：标记 shadow memory 脏区域，
    // draw 前 sync_persistent_buffer_if_needed 用 glBufferSubData 同步到 GLES buffer。
    // GL_PARAMETER_BUFFER 的 gles_buffer_id=0，sync 时 dirty_length 为 0 不会同步。
    let marked = state::with_state(|s| {
        let desktop_id = s.bound_buffers_by_target.get(&target).copied()?;
        let pm = s.persistent_buffers.get_mut(&desktop_id)?;
        let off = if offset > 0 { offset as usize } else { 0 };
        let len = if length > 0 { length as usize } else { 0 };
        if off + len > pm.shadow_size {
            return None;
        }
        // 合并脏区域：若与现有脏区域重叠或相邻则合并，否则取新区域。
        // Sodium 通常每帧 flush 相同区域，这里简化为"若已有脏区域则扩展为并集"。
        if pm.dirty_length == 0 {
            pm.dirty_offset = off;
            pm.dirty_length = len;
        } else {
            let existing_start = pm.dirty_offset;
            let existing_end = pm.dirty_offset + pm.dirty_length;
            let new_start = off.min(existing_start);
            let new_end = (off + len).max(existing_end);
            pm.dirty_offset = new_start;
            pm.dirty_length = new_end - new_start;
        }
        Some(())
    });
    if marked.is_some() {
        log::debug!(
            "[FluorateGL] glFlushMappedBufferRange(0x{:04X}): shadow dirty offset={} length={}",
            target,
            offset,
            length
        );
        return;
    }

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

        // 记录 target → desktop buffer 映射 + 持久映射条目的 bound_target
        // （UBO 通常经 BindBufferBase 绑定，必须记录否则 sync 无法定位）
        state::with_state(|s| {
            s.bound_buffers_by_target.insert(target, buffer);
            if let Some(pm) = s.persistent_buffers.get_mut(&buffer) {
                pm.bound_target = target;
            } else if buffer == 0 {
                for pm in s.persistent_buffers.values_mut() {
                    if pm.bound_target == target {
                        pm.bound_target = 0;
                    }
                }
            }
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

        // 记录 target → desktop buffer 映射 + 持久映射条目的 bound_target
        state::with_state(|s| {
            s.bound_buffers_by_target.insert(target, buffer);
            if let Some(pm) = s.persistent_buffers.get_mut(&buffer) {
                pm.bound_target = target;
            } else if buffer == 0 {
                for pm in s.persistent_buffers.values_mut() {
                    if pm.bound_target == target {
                        pm.bound_target = 0;
                    }
                }
            }
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
    // GL_BUFFER_MAP_POINTER（0x88BD）：持久映射（shadow）buffer 应返回 shadow_ptr。
    // shadow 路径下 GLES buffer 从未被 map，直通会返回 null，宿主会误判"未映射"。
    const GL_BUFFER_MAP_POINTER: u32 = 0x88BD;
    if pname == GL_BUFFER_MAP_POINTER {
        if let Some(ptr) = state::with_state_ref(|s| {
            let desktop_id = s.bound_buffers_by_target.get(&target).copied()?;
            s.persistent_buffers
                .get(&desktop_id)
                .map(|pm| pm.shadow_ptr as *mut std::ffi::c_void)
        }) {
            unsafe { *params = ptr };
            return;
        }
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
