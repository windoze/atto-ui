//! Dirty-signal collection plus the render / action plumbing shared by the
//! inspector facade and the change tracker.
//!
//! The render helper [`draw_desktop`] lives here (rather than under a separate
//! `render` module) because the dirty-signal collectors and the wait-loop poll
//! are its primary callers, and keeping them together avoids a one-function
//! module.

use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use crate::app::{Desktop, MenuItem};
use crate::composable::Component;
use crate::reactive::DirtySignal;
use crate::wm::Window;
use crate::ComponentError;

pub(super) fn poll_interval() -> Duration {
    Duration::from_millis(10)
}

pub(super) fn apply_desktop_action(desktop: &mut Desktop, action: &crate::app::DesktopAction) {
    if let crate::app::DesktopAction::CloseWindow(id) = *action {
        desktop.wm.close(id);
    }
}

pub(super) fn draw_desktop(
    desktop: &mut Desktop,
    screen: Rect,
) -> Result<Terminal<TestBackend>, ComponentError> {
    let backend = TestBackend::new(screen.width, screen.height);
    let mut terminal = Terminal::new(backend).map_err(ComponentError::render_failed)?;
    terminal
        .draw(|f| desktop.draw(f))
        .map_err(ComponentError::render_failed)?;
    Ok(terminal)
}

pub(super) fn collect_desktop_dirty_signals(desktop: &Desktop) -> Vec<DirtySignal> {
    let mut signals = Vec::new();
    collect_menu_dirty_signals(&desktop.menu, &mut signals);
    signals.extend(desktop.status.dirty_signals());
    for window in desktop.wm.windows() {
        collect_window_dirty_signals(window, &mut signals);
    }
    signals
}

fn collect_menu_dirty_signals(menu: &crate::app::MenuBar, signals: &mut Vec<DirtySignal>) {
    for spec in menu.menus() {
        signals.push(spec.title.dirty_signal());
        collect_menu_item_dirty_signals(&spec.items, signals);
    }
}

fn collect_menu_item_dirty_signals(items: &[MenuItem], signals: &mut Vec<DirtySignal>) {
    for item in items {
        signals.push(item.label.dirty_signal());
        signals.push(item.shortcut.dirty_signal());
        signals.push(item.accelerator.dirty_signal());
        signals.push(item.mnemonic.dirty_signal());
        signals.push(item.enabled.dirty_signal());
        collect_menu_item_dirty_signals(&item.submenu, signals);
    }
}

fn collect_window_dirty_signals(window: &Window, signals: &mut Vec<DirtySignal>) {
    signals.push(window.title.dirty_signal());
    signals.push(window.rect.dirty_signal());
    signals.push(window.state.dirty_signal());
    signals.push(window.dock.dirty_signal());
    signals.push(window.decorations.dirty_signal());
    signals.push(window.min_size.dirty_signal());
    signals.push(window.min_size_mode.dirty_signal());
    signals.push(window.movable.dirty_signal());
    signals.push(window.resizable.dirty_signal());
    signals.push(window.closable.dirty_signal());
    collect_component_dirty_signals(window.view.as_ref(), signals);
}

fn collect_component_dirty_signals(view: &dyn Component, signals: &mut Vec<DirtySignal>) {
    signals.extend(view.dirty_signals());
    for child in view.children() {
        collect_component_dirty_signals(child.view.as_ref(), signals);
    }
}
