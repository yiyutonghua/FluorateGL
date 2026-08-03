use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// 桌面 ID 全局唯一分配器：跨线程共享，从 1 开始单调递增。
/// 每线程的 [`IdMap`] 实例（thread_local State）从同一计数器中取号，
/// 保证多线程下桌面 ID 不碰撞。
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

pub struct IdMap {
    desktop_to_gles: FxHashMap<u32, u32>,
    gles_to_desktop: FxHashMap<u32, u32>,
}

impl IdMap {
    pub fn new() -> Self {
        Self {
            desktop_to_gles: FxHashMap::default(),
            gles_to_desktop: FxHashMap::default(),
        }
    }

    pub fn alloc(&mut self, gles_id: u32) -> u32 {
        let desktop_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        debug_assert!(desktop_id != 0, "NEXT_ID wrapped");
        self.desktop_to_gles.insert(desktop_id, gles_id);
        self.gles_to_desktop.insert(gles_id, desktop_id);
        desktop_id
    }

    pub fn get_gles(&self, desktop_id: u32) -> Option<u32> {
        self.desktop_to_gles.get(&desktop_id).copied()
    }

    pub fn get_desktop(&self, gles_id: u32) -> Option<u32> {
        self.gles_to_desktop.get(&gles_id).copied()
    }

    pub fn delete(&mut self, desktop_id: u32) -> Option<u32> {
        let gles_id = self.desktop_to_gles.remove(&desktop_id)?;
        self.gles_to_desktop.remove(&gles_id);
        Some(gles_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 多线程并发 alloc：桌面 ID 由全局 NEXT_ID 分配，必须全部唯一。
    #[test]
    fn alloc_ids_globally_unique_across_threads() {
        const THREADS: usize = 8;
        const ALLOCS_PER_THREAD: u32 = 1000;

        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                std::thread::spawn(|| {
                    // 每线程独立的 IdMap 实例（模拟 thread_local State），
                    // 各分配 1000 个 id，gles_id 用 1..=1000 即可（无需真实驱动）。
                    let mut map = IdMap::new();
                    (1..=ALLOCS_PER_THREAD)
                        .map(|gles_id| map.alloc(gles_id))
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        let ids: Vec<u32> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();

        assert_eq!(ids.len(), THREADS * ALLOCS_PER_THREAD as usize);
        assert!(ids.iter().all(|&id| id != 0), "desktop ID 不允许为 0");
        let unique: HashSet<u32> = ids.iter().copied().collect();
        assert_eq!(
            unique.len(),
            ids.len(),
            "desktop ID 发生跨线程碰撞: 共 {} 个, 去重后 {} 个",
            ids.len(),
            unique.len()
        );
    }
}
