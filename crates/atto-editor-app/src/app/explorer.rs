//! Explorer dock management: dock-state sync, dock config, toggle, and
//! dock-window placement.

use super::*;

pub(crate) fn sync_explorer_dock_state(desktop: &Desktop, state: &Arc<Mutex<AppState>>) {
    let Some(id) = state.lock().explorer_window else {
        return;
    };

    let Some(window) = desktop.wm.window(id) else {
        state.lock().explorer_window = None;
        return;
    };

    if let Some(dock) = window.dock.get() {
        let mut s = state.lock();
        s.explorer_dock = dock.side;
        s.explorer_size = Some(dock.size);
    }
}

pub(crate) fn explorer_dock_config(side: DockSide, size: u16) -> WindowDock {
    WindowDock {
        side,
        size,
        min_size: MIN_EXPLORER_DOCK_SIZE,
        max_size: None,
        auto_hide: DockAutoHide::Disabled,
        handle_label: Some("Explorer".to_string()),
    }
}

pub(crate) fn toggle_explorer_window(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: EventQueue<AppAction>,
) {
    // Close if currently open.
    let open_id = state.lock().explorer_window;
    if let Some(id) = open_id {
        if let Some(w) = desktop.wm.window(id) {
            if let Some(dock) = w.dock.get() {
                let mut s = state.lock();
                s.explorer_dock = dock.side;
                s.explorer_size = Some(dock.size);
            }
            desktop.wm.close(id);
            state.lock().explorer_window = None;
            return;
        }

        // Window is gone (closed elsewhere).
        state.lock().explorer_window = None;
    }

    // Open a new Explorer window.
    let (side, size, roots, explorer_cmds) = {
        let s = state.lock();
        (
            s.explorer_dock,
            s.explorer_size.unwrap_or(DEFAULT_EXPLORER_DOCK_SIZE),
            s.workspace_roots.clone(),
            s.explorer_commands.clone(),
        )
    };

    let view = ExplorerWindowView::new(actions.clone(), explorer_cmds, roots);
    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Explorer",
            Rect::default(),
            Box::new(view),
        )
        .with_dock(Some(explorer_dock_config(side, size)))
        .with_tag("atto-editor-app-explorer")
        .with_close_hook({
            let state = state.clone();
            move |id| {
                let mut s = state.lock();
                if s.explorer_window == Some(id) {
                    s.explorer_window = None;
                }
                true
            }
        }),
        screen,
    );

    let mut s = state.lock();
    s.explorer_window = Some(id);
    s.explorer_dock = side;
    s.explorer_size = Some(size);
}

pub(crate) fn dock_explorer_window(
    desktop: &mut Desktop,
    _screen: Rect,
    state: &Arc<Mutex<AppState>>,
    side: DockSide,
) {
    let (size, id) = {
        let s = state.lock();
        (
            s.explorer_size.unwrap_or(DEFAULT_EXPLORER_DOCK_SIZE),
            s.explorer_window,
        )
    };

    {
        let mut s = state.lock();
        s.explorer_dock = side;
        s.explorer_size = Some(size);
    }

    if let Some(id) = id {
        if let Some(w) = desktop.wm.window_mut(id) {
            let mut dock = w
                .dock
                .get()
                .unwrap_or_else(|| explorer_dock_config(side, size));
            dock.side = side;
            dock.size = size;
            dock.min_size = MIN_EXPLORER_DOCK_SIZE;
            dock.auto_hide = DockAutoHide::Disabled;
            if dock.handle_label.is_none() {
                dock.handle_label = Some("Explorer".to_string());
            }
            w.dock.set(Some(dock));
        } else {
            state.lock().explorer_window = None;
        }
    }
}
