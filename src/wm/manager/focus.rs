// Focus management helpers for WindowManager.

use super::{WindowId, WindowManager, WindowState, chrome};

impl WindowManager {
    pub fn has_active_modal(&self) -> bool {
        self.active_modal_id().is_some()
    }

    pub fn focused(&self) -> Option<WindowId> {
        self.active_modal_id().or(self.focused)
    }

    pub fn focus(&mut self, id: WindowId) {
        if self.active_modal_id().is_some() {
            return;
        }
        if !self.window(id).is_some_and(|w| w.kind.is_focusable()) {
            return;
        }
        self.focused = Some(id);
        self.bring_to_front(id);
    }

    pub fn focus_next(&mut self) {
        if self.active_modal_id().is_some() {
            return;
        }
        let ids: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.kind.is_focusable() && w.state.get() != WindowState::Minimized)
            .map(|w| w.id)
            .collect();
        if ids.is_empty() {
            self.focused = None;
            return;
        }
        let current = self.focused;
        let next = match current.and_then(|c| ids.iter().position(|id| *id == c)) {
            Some(idx) => ids[(idx + 1) % ids.len()],
            None => ids[0],
        };
        self.focused = Some(next);
        self.bring_to_front(next);
    }

    pub fn minimize_focused(&mut self) {
        let Some(id) = self.focused() else { return };
        let can_minimize = self.window(id).is_some_and(chrome::can_minimize);
        if !can_minimize {
            return;
        }
        if let Some(w) = self.window_mut(id) {
            w.state.set(WindowState::Minimized);
        }
        self.focused = self.topmost_focusable_id();
    }

    pub fn restore_focused(&mut self) {
        let Some(id) = self.focused() else { return };
        if let Some(w) = self.window_mut(id)
            && w.state.get() == WindowState::Minimized
        {
            w.state.set(WindowState::Normal);
        }
    }

    pub fn restore_window(&mut self, id: WindowId) -> bool {
        let restored = if let Some(w) = self.window_mut(id)
            && w.state.get() == WindowState::Minimized
        {
            w.state.set(WindowState::Normal);
            true
        } else {
            false
        };
        if restored {
            self.focus(id);
        }
        restored
    }

    pub(super) fn topmost_focusable_id(&self) -> Option<WindowId> {
        self.windows
            .iter()
            .rev()
            .find(|w| w.kind.is_focusable() && w.state.get() != WindowState::Minimized)
            .map(|w| w.id)
    }

    pub(super) fn active_modal_id(&self) -> Option<WindowId> {
        self.windows
            .iter()
            .rev()
            .find(|w| w.kind.is_modal() && w.state.get() != WindowState::Minimized)
            .map(|w| w.id)
    }
}
