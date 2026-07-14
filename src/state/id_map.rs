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

    pub fn delete(&mut self, desktop_id: u32) -> Option<u32> {
        let gles_id = self.desktop_to_gles.remove(&desktop_id)?;
        self.gles_to_desktop.remove(&gles_id);
        Some(gles_id)
    }
}
