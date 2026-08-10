use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::atomic::{AtomicU32, Ordering};

/// 桌面 ID 全局唯一分配器：跨线程共享，从 1 开始单调递增。
/// 每线程的 [`IdMap`] 实例（thread_local State）从同一计数器中取号，
/// 保证多线程下桌面 ID 不碰撞。
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// 双向 ID 映射（支持 lazy 后端对象创建）
///
/// 桌面 GL 与底层 GLES 的对象 ID 是两套独立命名空间。`alloc` 立即绑定两端；
/// `alloc_pending` 只分配桌面 ID、暂不创建后端对象（MG gen_buffer 惰性语义：
/// 宿主 gen 大量 buffer 名但多数不使用，未使用的对象永不触碰驱动——
/// Adreno 部分版本大量 gen 未用 buffer 会崩溃）。首次真实使用时经
/// `bind_gles` 完成映射。
pub struct IdMap {
    desktop_to_gles: FxHashMap<u32, u32>,
    gles_to_desktop: FxHashMap<u32, u32>,
    /// 已分配但尚未创建后端对象的桌面 ID（lazy pending 集合）
    pending: FxHashSet<u32>,
}

impl IdMap {
    pub fn new() -> Self {
        Self {
            desktop_to_gles: FxHashMap::default(),
            gles_to_desktop: FxHashMap::default(),
            pending: FxHashSet::default(),
        }
    }

    pub fn alloc(&mut self, gles_id: u32) -> u32 {
        let desktop_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        debug_assert!(desktop_id != 0, "NEXT_ID wrapped");
        self.desktop_to_gles.insert(desktop_id, gles_id);
        self.gles_to_desktop.insert(gles_id, desktop_id);
        desktop_id
    }

    /// 只分配桌面 ID，不创建后端对象（MG gen_buffer 惰性语义）。
    /// 首次真实使用（bind/upload/map/delete 等）由调用方 `bind_gles` 完成映射。
    pub fn alloc_pending(&mut self) -> u32 {
        let desktop_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        debug_assert!(desktop_id != 0, "NEXT_ID wrapped");
        self.pending.insert(desktop_id);
        desktop_id
    }

    /// 懒创建：将桌面 ID 绑定到后端对象（幂等——重复绑定覆盖旧映射并清理反向表）。
    pub fn bind_gles(&mut self, desktop_id: u32, gles_id: u32) {
        self.pending.remove(&desktop_id);
        if let Some(old) = self.desktop_to_gles.insert(desktop_id, gles_id) {
            self.gles_to_desktop.remove(&old);
        }
        self.gles_to_desktop.insert(gles_id, desktop_id);
    }

    /// 桌面 ID 是否已登记（无论是否已创建后端对象）
    pub fn contains(&self, desktop_id: u32) -> bool {
        self.desktop_to_gles.contains_key(&desktop_id) || self.pending.contains(&desktop_id)
    }

    #[allow(dead_code)]
    /// 是否已创建后端对象（`alloc_pending` 后为 false，`bind_gles` 后为 true）
    pub fn has_gles(&self, desktop_id: u32) -> bool {
        self.desktop_to_gles.contains_key(&desktop_id)
    }

    pub fn get_gles(&self, desktop_id: u32) -> Option<u32> {
        self.desktop_to_gles.get(&desktop_id).copied()
    }

    pub fn get_desktop(&self, gles_id: u32) -> Option<u32> {
        self.gles_to_desktop.get(&gles_id).copied()
    }

    /// 删除记录：pending（未创建后端对象）返回 None；已创建返回其 gles id 供调用方释放。
    pub fn delete(&mut self, desktop_id: u32) -> Option<u32> {
        self.pending.remove(&desktop_id);
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

    /// lazy 语义：alloc_pending 后登记但不创建后端对象；bind_gles 幂等完成映射；
    /// 反向映射随覆盖/删除正确清理。
    #[test]
    fn lazy_pending_semantics() {
        let mut map = IdMap::new();
        let id = map.alloc_pending();
        assert!(map.contains(id), "pending 分配后应已登记");
        assert!(!map.has_gles(id), "pending 尚未创建后端对象");
        assert_eq!(map.get_gles(id), None, "pending 无 gles 映射");

        // 懒创建：首次真实使用绑定
        map.bind_gles(id, 42);
        assert!(map.contains(id));
        assert!(map.has_gles(id));
        assert_eq!(map.get_gles(id), Some(42));
        assert_eq!(map.get_desktop(42), Some(id));

        // 已创建对象删除：返回 gles id 供调用方释放
        assert_eq!(map.delete(id), Some(42));
        assert!(!map.contains(id));
        assert_eq!(map.get_desktop(42), None);

        // bind_gles 幂等：覆盖旧映射并清理旧反向项
        map.bind_gles(id, 7);
        map.bind_gles(id, 8);
        assert_eq!(map.get_gles(id), Some(8));
        assert_eq!(map.get_desktop(7), None, "旧映射应被清理");
        assert_eq!(map.get_desktop(8), Some(id));
    }

    /// 永不使用的 pending 删除：返回 None（无后端对象可删），记录清除。
    #[test]
    fn pending_delete_and_never_created() {
        let mut map = IdMap::new();
        let unused = map.alloc_pending();
        let used = map.alloc_pending();
        map.bind_gles(used, 7);

        // 永不使用的 buffer：delete 返回 None（从未创建 GLES 对象，无需释放）
        assert_eq!(map.delete(unused), None);
        assert!(!map.contains(unused));
        assert!(!map.has_gles(unused));

        assert_eq!(map.delete(used), Some(7));
        assert!(!map.contains(used));
    }
}
