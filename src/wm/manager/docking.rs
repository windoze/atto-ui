// Docking layout helpers for WindowManager.

use ratatui::layout::Rect;

use super::{DockAutoHide, DockSide, Window, WindowDock, WindowManager, WindowState};

pub(crate) fn dock_rect(bounds: Rect, dock: &WindowDock, reserved_before: Rect) -> Rect {
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

pub(crate) fn reserve_for_docked_windows(windows: &[Window], bounds: Rect) -> Rect {
    let mut reserved = bounds;
    for window in windows {
        let Some(dock) = active_dock(window) else {
            continue;
        };
        let rect = dock_rect(bounds, &dock, reserved);
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
            reserved = reserve_rect(reserved, dock.side, rect);
        }
        reserved
    }
}

fn active_dock(window: &Window) -> Option<WindowDock> {
    if window.state.get() == WindowState::Minimized {
        return None;
    }
    window.dock.get()
}

fn dock_reserve_size(dock: &WindowDock, area: Rect) -> u16 {
    let available = match dock.side {
        DockSide::Left | DockSide::Right => area.width,
        DockSide::Bottom | DockSide::Top => area.height,
    };
    if available == 0 {
        return 0;
    }

    if matches!(dock.auto_hide, DockAutoHide::Enabled { visible: false }) {
        return 1.min(available);
    }

    let min_size = dock.min_size.min(available);
    let max_size = dock
        .max_size
        .unwrap_or(available)
        .min(available)
        .max(min_size);
    dock.size.clamp(min_size, max_size)
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
