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

    /// Raise `id` as high as possible without covering an active modal.
    ///
    /// Windows are drawn in vec order (later = on top), so `bring_to_front` (push to the end) would
    /// place a newly added plain window *above* a modal that is already showing — the modal becomes
    /// visually blocked even though input is still trapped inside it, leaving a "stuck" screen.
    /// Instead, move `id` to just below the lowest modal window; with no modal present this behaves
    /// exactly like `bring_to_front`.
    pub(super) fn raise_below_modals(&mut self, id: WindowId) {
        let Some(pos) = self.window_index_of(id) else {
            return;
        };
        let w = self.windows.remove(pos);
        let insert_at = self
            .windows
            .iter()
            .position(|other| other.kind.is_modal())
            .unwrap_or(self.windows.len());
        self.windows.insert(insert_at, w);
        self.rebuild_window_index();
    }
}
