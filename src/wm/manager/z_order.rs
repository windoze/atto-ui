// Z-order management helpers for WindowManager.

use super::{WindowId, WindowManager};

impl WindowManager {
    pub fn bring_to_front(&mut self, id: WindowId) {
        let Some(pos) = self.window_index_of(id) else {
            return;
        };
        let w = self.windows.remove(pos);
        self.windows.push(w);
        self.rebuild_window_index();
    }
}
