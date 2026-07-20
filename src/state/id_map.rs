use std::collections::HashMap;

pub struct IdMap {
    next_id: u32,
    desktop_to_gles: HashMap<u32, u32>,
    gles_to_desktop: HashMap<u32, u32>,
}

impl IdMap {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            desktop_to_gles: HashMap::new(),
            gles_to_desktop: HashMap::new(),
        }
    }

    pub fn alloc(&mut self, gles_id: u32) -> u32 {
        let desktop_id = self.next_id;
        self.next_id += 1;
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

    #[test]
    fn alloc_assigns_distinct_desktop_ids() {
        let mut m = IdMap::new();
        let a = m.alloc(100);
        let b = m.alloc(200);
        let c = m.alloc(300);
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn get_gles_returns_allocated_id() {
        let mut m = IdMap::new();
        let desktop = m.alloc(42);
        assert_eq!(m.get_gles(desktop), Some(42));
    }

    #[test]
    fn get_desktop_returns_reverse_mapping() {
        let mut m = IdMap::new();
        let desktop = m.alloc(7);
        assert_eq!(m.get_desktop(7), Some(desktop));
    }

    #[test]
    fn get_on_unknown_id_returns_none() {
        let m = IdMap::new();
        assert_eq!(m.get_gles(999), None);
        assert_eq!(m.get_desktop(999), None);
    }

    #[test]
    fn delete_removes_both_directions() {
        let mut m = IdMap::new();
        let desktop = m.alloc(11);
        let removed = m.delete(desktop);
        assert_eq!(removed, Some(11));
        assert_eq!(m.get_gles(desktop), None);
        assert_eq!(m.get_desktop(11), None);
    }

    #[test]
    fn delete_unknown_returns_none() {
        let mut m = IdMap::new();
        assert_eq!(m.delete(0), None);
        assert_eq!(m.delete(u32::MAX), None);
    }

    #[test]
    fn desktop_ids_keep_increasing_after_delete() {
        let mut m = IdMap::new();
        let a = m.alloc(1);
        let _ = m.delete(a);
        let b = m.alloc(2);
        // IdMap 不复用已删除的 desktop id，单调递增
        assert!(b > a);
    }
}
