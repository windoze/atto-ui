// Docking layout helpers for WindowManager.

use ratatui::layout::Rect;

use super::{DockAutoHide, DockSide, Window, WindowDock, WindowId, WindowManager, WindowState};

pub(crate) fn dock_rect(bounds: Rect, dock: &WindowDock, reserved_before: Rect) -> Rect {
    let area = clip_rect(reserved_before, bounds);
    let size = dock_visible_size(dock, area);

    match dock.side {
        DockSide::Left => Rect::new(area.x, area.y, size, area.height),
        DockSide::Right => Rect::new(
            area.x.saturating_add(area.width).saturating_sub(size),
            area.y,
            size,
            area.height,
        ),
        DockSide::Bottom => Rect::new(
            area.x,
            area.y.saturating_add(area.height).saturating_sub(size),
            area.width,
            size,
        ),
        DockSide::Top => Rect::new(area.x, area.y, area.width, size),
    }
}

pub(super) fn dock_resize_edge_rect(window_rect: Rect, side: DockSide) -> Rect {
    if window_rect.width == 0 || window_rect.height == 0 {
        return Rect::new(window_rect.x, window_rect.y, 0, 0);
    }

    match side {
        DockSide::Left => Rect::new(
            window_rect
                .x
                .saturating_add(window_rect.width)
                .saturating_sub(1),
            window_rect.y,
            1,
            window_rect.height,
        ),
        DockSide::Right => Rect::new(window_rect.x, window_rect.y, 1, window_rect.height),
        DockSide::Bottom => Rect::new(window_rect.x, window_rect.y, window_rect.width, 1),
        DockSide::Top => Rect::new(
            window_rect.x,
            window_rect
                .y
                .saturating_add(window_rect.height)
                .saturating_sub(1),
            window_rect.width,
            1,
        ),
    }
}

pub(super) fn dock_handle_rect(window_rect: Rect, dock: &WindowDock) -> Rect {
    if window_rect.width == 0 || window_rect.height == 0 {
        return Rect::new(window_rect.x, window_rect.y, 0, 0);
    }

    match dock.side {
        DockSide::Left => Rect::new(window_rect.x, window_rect.y, 1, window_rect.height),
        DockSide::Right => Rect::new(
            window_rect
                .x
                .saturating_add(window_rect.width)
                .saturating_sub(1),
            window_rect.y,
            1,
            window_rect.height,
        ),
        DockSide::Bottom => Rect::new(
            window_rect.x,
            window_rect
                .y
                .saturating_add(window_rect.height)
                .saturating_sub(1),
            window_rect.width,
            1,
        ),
        DockSide::Top => Rect::new(window_rect.x, window_rect.y, window_rect.width, 1),
    }
}

pub(super) fn dock_size_from_pointer(
    area: Rect,
    side: DockSide,
    pointer_x: u16,
    pointer_y: u16,
    fallback_size: u16,
) -> u16 {
    match side {
        DockSide::Left => {
            if area.width == 0 {
                fallback_size
            } else {
                pointer_x.saturating_sub(area.x).saturating_add(1)
            }
        }
        DockSide::Right => {
            if area.width == 0 {
                fallback_size
            } else {
                let right = area.x.saturating_add(area.width).saturating_sub(1);
                right.saturating_sub(pointer_x).saturating_add(1)
            }
        }
        DockSide::Bottom => {
            if area.height == 0 {
                fallback_size
            } else {
                let bottom = area.y.saturating_add(area.height).saturating_sub(1);
                bottom.saturating_sub(pointer_y).saturating_add(1)
            }
        }
        DockSide::Top => {
            if area.height == 0 {
                fallback_size
            } else {
                pointer_y.saturating_sub(area.y).saturating_add(1)
            }
        }
    }
}

pub(super) fn clamp_dock_size(dock: &WindowDock, area: Rect, size: u16) -> u16 {
    let available = dock_available_size(dock.side, area);
    if available == 0 {
        return 0;
    }

    let min_size = dock.min_size.min(available);
    let max_size = dock
        .max_size
        .unwrap_or(available)
        .min(available)
        .max(min_size);
    size.clamp(min_size, max_size)
}

pub(crate) fn reserve_for_docked_windows(windows: &[Window], bounds: Rect) -> Rect {
    let mut reserved = bounds;
    for window in windows {
        let Some(dock) = active_dock(window) else {
            continue;
        };
        let rect = dock_reserve_rect(bounds, &dock, reserved);
        reserved = reserve_rect(reserved, dock.side, rect);
    }
    reserved
}

impl WindowManager {
    pub fn effective_work_area(&self, bounds: Rect) -> Rect {
        reserve_for_docked_windows(&self.windows, bounds)
    }

    pub(super) fn apply_dock_layout(&mut self, bounds: Rect) -> Rect {
        let mut reserved = bounds;
        for window in &mut self.windows {
            let Some(dock) = active_dock(window) else {
                continue;
            };
            window.movable.set(false);
            window.resizable.set(true);
            window.state.set(WindowState::Normal);
            let rect = dock_rect(bounds, &dock, reserved);
            window.rect.set(rect);
            let reserve = dock_reserve_rect(bounds, &dock, reserved);
            reserved = reserve_rect(reserved, dock.side, reserve);
        }
        reserved
    }

    pub(super) fn dock_area_for_window(&self, id: WindowId, bounds: Rect) -> Rect {
        let mut reserved = bounds;
        for window in &self.windows {
            if window.id == id {
                return clip_rect(reserved, bounds);
            }
            let Some(dock) = active_dock(window) else {
                continue;
            };
            let rect = dock_reserve_rect(bounds, &dock, reserved);
            reserved = reserve_rect(reserved, dock.side, rect);
        }
        clip_rect(reserved, bounds)
    }

    pub(super) fn hide_auto_hide_docks_except(&mut self, keep: Option<WindowId>) -> bool {
        let mut changed = false;
        for window in &mut self.windows {
            if Some(window.id) == keep {
                continue;
            }
            let Some(mut dock) = window.dock.get() else {
                continue;
            };
            if matches!(dock.auto_hide, DockAutoHide::Enabled { visible: true }) {
                let handle_rect = dock_handle_rect(window.rect.get(), &dock);
                dock.auto_hide = DockAutoHide::Enabled { visible: false };
                window.dock.set(Some(dock));
                window.rect.set(handle_rect);
                changed = true;
            }
        }
        changed
    }
}

pub(super) fn window_is_auto_hide_dock(window: &Window) -> bool {
    window
        .dock
        .get()
        .is_some_and(|dock| matches!(dock.auto_hide, DockAutoHide::Enabled { .. }))
}

pub(super) fn window_is_visible_auto_hide_dock(window: &Window) -> bool {
    window
        .dock
        .get()
        .is_some_and(|dock| matches!(dock.auto_hide, DockAutoHide::Enabled { visible: true }))
}

fn active_dock(window: &Window) -> Option<WindowDock> {
    if window.state.get() == WindowState::Minimized {
        return None;
    }
    window.dock.get()
}

fn dock_reserve_rect(bounds: Rect, dock: &WindowDock, reserved_before: Rect) -> Rect {
    let area = clip_rect(reserved_before, bounds);
    let size = dock_reserve_size(dock, area);

    match dock.side {
        DockSide::Left => Rect::new(area.x, area.y, size, area.height),
        DockSide::Right => Rect::new(
            area.x.saturating_add(area.width).saturating_sub(size),
            area.y,
            size,
            area.height,
        ),
        DockSide::Bottom => Rect::new(
            area.x,
            area.y.saturating_add(area.height).saturating_sub(size),
            area.width,
            size,
        ),
        DockSide::Top => Rect::new(area.x, area.y, area.width, size),
    }
}

fn dock_visible_size(dock: &WindowDock, area: Rect) -> u16 {
    if matches!(dock.auto_hide, DockAutoHide::Enabled { visible: false }) {
        return 1.min(dock_available_size(dock.side, area));
    }

    clamp_dock_size(dock, area, dock.size)
}

fn dock_reserve_size(dock: &WindowDock, area: Rect) -> u16 {
    let available = dock_available_size(dock.side, area);
    if available == 0 {
        return 0;
    }

    if matches!(dock.auto_hide, DockAutoHide::Enabled { .. }) {
        return 1.min(available);
    }

    clamp_dock_size(dock, area, dock.size)
}

fn dock_available_size(side: DockSide, area: Rect) -> u16 {
    match side {
        DockSide::Left | DockSide::Right => area.width,
        DockSide::Bottom | DockSide::Top => area.height,
    }
}

fn reserve_rect(mut area: Rect, side: DockSide, rect: Rect) -> Rect {
    match side {
        DockSide::Left => {
            let used = rect.width.min(area.width);
            area.x = area.x.saturating_add(used);
            area.width = area.width.saturating_sub(used);
        }
        DockSide::Right => {
            let used = rect.width.min(area.width);
            area.width = area.width.saturating_sub(used);
        }
        DockSide::Bottom => {
            let used = rect.height.min(area.height);
            area.height = area.height.saturating_sub(used);
        }
        DockSide::Top => {
            let used = rect.height.min(area.height);
            area.y = area.y.saturating_add(used);
            area.height = area.height.saturating_sub(used);
        }
    }
    area
}

fn clip_rect(rect: Rect, bounds: Rect) -> Rect {
    let x0 = rect.x.max(bounds.x);
    let y0 = rect.y.max(bounds.y);
    let x1 = rect
        .x
        .saturating_add(rect.width)
        .min(bounds.x.saturating_add(bounds.width));
    let y1 = rect
        .y
        .saturating_add(rect.height)
        .min(bounds.y.saturating_add(bounds.height));

    if x1 <= x0 || y1 <= y0 {
        return Rect::new(x0, y0, 0, 0);
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}
