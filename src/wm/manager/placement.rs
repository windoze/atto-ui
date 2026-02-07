// Placement and geometry helpers for WindowManager.

use ratatui::layout::Rect;

use super::{
    ResizeCorner, Window, WindowId, WindowManager, WindowMinSizeMode, WindowState, chrome,
};

impl WindowManager {
    pub fn move_focused(&mut self, dx: i16, dy: i16, bounds: Rect) {
        let Some(id) = self.focused() else { return };
        self.move_window(id, dx, dy, bounds);
    }

    pub fn resize_focused(&mut self, dw: i16, dh: i16, bounds: Rect) {
        let Some(id) = self.focused() else { return };
        self.resize_window(id, dw, dh, bounds);
    }

    pub fn toggle_maximize_focused(&mut self, bounds: Rect) {
        let Some(id) = self.focused() else { return };
        let can_toggle = self
            .windows
            .iter()
            .any(|w| w.id == id && chrome::can_toggle_maximize(w));
        if can_toggle {
            self.toggle_maximize(id, bounds);
        }
    }

    fn move_window(&mut self, id: WindowId, dx: i16, dy: i16, bounds: Rect) {
        let Some(w) = self.window_mut(id) else { return };
        if !w.movable.get() || w.state.get() == WindowState::Maximized {
            return;
        }
        let mut rect = w.rect.get();
        rect.x = add_signed(rect.x, dx);
        rect.y = add_signed(rect.y, dy);
        w.rect
            .set(normalize_rect(rect, bounds, window_enforced_min_size(w)));
    }

    fn resize_window(&mut self, id: WindowId, dw: i16, dh: i16, bounds: Rect) {
        let Some(w) = self.window_mut(id) else { return };
        if !w.resizable.get() || w.state.get() == WindowState::Maximized {
            return;
        }
        let enforced_min_size = window_enforced_min_size(w);
        let (min_w, min_h) = enforced_min_size;
        let mut rect = w.rect.get();
        rect.width = add_signed(rect.width, dw).max(min_w);
        rect.height = add_signed(rect.height, dh).max(min_h);
        w.rect.set(normalize_rect(rect, bounds, enforced_min_size));
    }

    pub(super) fn toggle_maximize(&mut self, id: WindowId, bounds: Rect) {
        let Some(w) = self.window_mut(id) else { return };
        match w.state.get() {
            WindowState::Maximized => {
                w.state.set(WindowState::Normal);
                if let Some(r) = w.restore_rect.take() {
                    w.rect
                        .set(normalize_rect(r, bounds, window_enforced_min_size(w)));
                }
            }
            WindowState::Normal => {
                w.restore_rect = Some(w.rect.get());
                w.state.set(WindowState::Maximized);
                w.rect.set(bounds);
            }
            WindowState::Minimized => {}
        }
    }
}

pub(super) fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn add_signed(v: u16, dv: i16) -> u16 {
    if dv.is_negative() {
        v.saturating_sub(dv.wrapping_abs() as u16)
    } else {
        v.saturating_add(dv as u16)
    }
}

fn window_effective_min_size(window: &Window) -> (u16, u16) {
    let (base_w, base_h) = window.min_size.get();
    let (view_min_w, view_min_h) = window.view.min_size();

    // Views are drawn into the inner rect (excluding window chrome). If the window is bordered,
    // add 1 cell on each side so the inner rect can still satisfy the view's minimum size.
    let decorations = window.decorations.get();
    let (chrome_w, chrome_h) = if decorations.border.has_border() {
        (2u16, 2u16)
    } else {
        (0u16, 0u16)
    };

    let required_w = view_min_w.saturating_add(chrome_w);
    let required_h = view_min_h.saturating_add(chrome_h);

    (base_w.max(required_w), base_h.max(required_h))
}

pub(super) fn window_enforced_min_size(window: &Window) -> (u16, u16) {
    match window.min_size_mode.get() {
        WindowMinSizeMode::Enforce => window_effective_min_size(window),
        WindowMinSizeMode::Clip | WindowMinSizeMode::Scroll => (1, 1),
    }
}

pub(super) fn normalize_rect(mut rect: Rect, bounds: Rect, min_size: (u16, u16)) -> Rect {
    let min_w = min_size.0.min(bounds.width);
    let min_h = min_size.1.min(bounds.height);

    rect.width = rect.width.max(min_w).min(bounds.width);
    rect.height = rect.height.max(min_h).min(bounds.height);

    let max_x = bounds
        .x
        .saturating_add(bounds.width.saturating_sub(rect.width));
    let max_y = bounds
        .y
        .saturating_add(bounds.height.saturating_sub(rect.height));

    rect.x = rect.x.clamp(bounds.x, max_x);
    rect.y = rect.y.clamp(bounds.y, max_y);
    rect
}

pub(super) fn resize_rect_from_corner(
    start: Rect,
    corner: ResizeCorner,
    pointer_x: u16,
    pointer_y: u16,
    bounds: Rect,
    min_size: (u16, u16),
) -> Rect {
    if start.width == 0 || start.height == 0 || bounds.width == 0 || bounds.height == 0 {
        return start;
    }

    let bounds_left = bounds.x;
    let bounds_top = bounds.y;
    let bounds_right = bounds.x.saturating_add(bounds.width).saturating_sub(1);
    let bounds_bottom = bounds.y.saturating_add(bounds.height).saturating_sub(1);

    let start_left = start.x;
    let start_top = start.y;
    let start_right = start.x.saturating_add(start.width).saturating_sub(1);
    let start_bottom = start.y.saturating_add(start.height).saturating_sub(1);

    let (left, right) = match corner {
        ResizeCorner::BottomRight | ResizeCorner::TopRight => {
            // Left is fixed.
            let max_w = bounds_right.saturating_sub(start_left).saturating_add(1);
            let min_w = min_size.0.min(max_w);
            let right_min = start_left.saturating_add(min_w).saturating_sub(1);
            (start_left, pointer_x.clamp(right_min, bounds_right))
        }
        ResizeCorner::BottomLeft | ResizeCorner::TopLeft => {
            // Right is fixed.
            let max_w = start_right.saturating_sub(bounds_left).saturating_add(1);
            let min_w = min_size.0.min(max_w);
            let left_max = start_right.saturating_sub(min_w).saturating_add(1);
            (pointer_x.clamp(bounds_left, left_max), start_right)
        }
    };

    let (top, bottom) = match corner {
        ResizeCorner::BottomRight | ResizeCorner::BottomLeft => {
            // Top is fixed.
            let max_h = bounds_bottom.saturating_sub(start_top).saturating_add(1);
            let min_h = min_size.1.min(max_h);
            let bottom_min = start_top.saturating_add(min_h).saturating_sub(1);
            (start_top, pointer_y.clamp(bottom_min, bounds_bottom))
        }
        ResizeCorner::TopRight | ResizeCorner::TopLeft => {
            // Bottom is fixed.
            let max_h = start_bottom.saturating_sub(bounds_top).saturating_add(1);
            let min_h = min_size.1.min(max_h);
            let top_max = start_bottom.saturating_sub(min_h).saturating_add(1);
            (pointer_y.clamp(bounds_top, top_max), start_bottom)
        }
    };

    Rect {
        x: left,
        y: top,
        width: right.saturating_sub(left).saturating_add(1),
        height: bottom.saturating_sub(top).saturating_add(1),
    }
}
