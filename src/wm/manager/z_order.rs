// Z-order management helpers for WindowManager.

use super::{WindowId, WindowManager};

impl WindowManager {
    pub fn bring_to_front(&mut self, id: WindowId) {
        let Some(pos) = self.windows.iter().position(|w| w.id == id) else {
            return;
        };
        let w = self.windows.remove(pos);
        self.windows.push(w);
    }
}
