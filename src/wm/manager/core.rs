use ratatui::layout::Rect;

use crate::composable::Component;
use crate::{TreeError, TreeOp};

use super::{Window, WindowId, WindowKind, WindowManager, docking, placement};

impl WindowManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn rebuild_window_index(&mut self) {
        self.window_index.clear();
        for (idx, window) in self.windows.iter().enumerate() {
            self.window_index.insert(window.id, idx);
        }
    }

    pub(super) fn window_index_of(&self, id: WindowId) -> Option<usize> {
        if let Some(idx) = self.window_index.get(&id).copied()
            && self.windows.get(idx).is_some_and(|window| window.id == id)
        {
            return Some(idx);
        }

        self.windows.iter().position(|window| window.id == id)
    }

    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    pub fn has_global_drag(&self) -> bool {
        self.global_drag.is_some()
    }

    pub(crate) fn windows_mut(&mut self) -> &mut [Window] {
        &mut self.windows
    }

    pub fn add_window(&mut self, mut window: Window, bounds: Rect) -> WindowId {
        self.next_id += 1;
        let id = WindowId(self.next_id);
        window.id = id;

        let enforced_min_size = placement::window_enforced_min_size(&window);
        let rect = if let Some(dock) = window.dock.get() {
            window.movable.set(false);
            window.resizable.set(true);
            window.state.set(super::WindowState::Normal);
            let reserved = self.effective_work_area(bounds);
            docking::dock_rect(bounds, &dock, reserved)
        } else {
            placement::normalize_rect(
                window.rect.get(),
                self.effective_work_area(bounds),
                enforced_min_size,
            )
        };
        window.rect.set(rect);

        if window.kind == WindowKind::Modal {
            // Ensure modals are always on top and focused.
            self.focused = Some(id);
        } else if window.kind.is_focusable() {
            self.focused = Some(id);
        }

        self.windows.push(window);
        self.rebuild_window_index();
        self.bring_to_front(id);
        id
    }

    pub fn close(&mut self, id: WindowId) {
        self.drag = match self.drag {
            Some(d) if d.window_id == id => None,
            other => other,
        };
        self.global_drag = match self.global_drag.take() {
            Some(d) if d.source_window == id || d.target_window == Some(id) => None,
            other => other,
        };
        self.mouse_capture = false;
        let was_focused = self.focused == Some(id);
        self.windows.retain(|w| w.id != id);
        self.rebuild_window_index();
        if was_focused {
            self.focused = self.topmost_focusable_id();
        }
    }

    pub fn request_close(&mut self, id: WindowId) -> bool {
        let allow = {
            let Some(w) = self.window_mut(id) else {
                return false;
            };
            w.allow_close()
        };
        if allow {
            self.close(id);
            true
        } else {
            false
        }
    }

    pub fn set_view(&mut self, id: WindowId, view: Box<dyn Component>) -> bool {
        let Some(window) = self.window_mut(id) else {
            return false;
        };
        window.set_view(view);
        true
    }

    pub fn apply_tree_ops(&mut self, id: WindowId, ops: &[TreeOp]) -> Result<bool, TreeError> {
        let Some(window) = self.window_mut(id) else {
            return Err(TreeError::NotFound(format!("window:{}", id.0)));
        };
        window.apply_tree_ops(ops)
    }

    pub fn rebuild_dynamic_window(&mut self, id: WindowId) -> Result<(), TreeError> {
        let Some(window) = self.window_mut(id) else {
            return Err(TreeError::NotFound(format!("window:{}", id.0)));
        };
        window.rebuild_dynamic()
    }
}
