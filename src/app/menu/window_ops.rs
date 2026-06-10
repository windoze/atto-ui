use crate::reactive::Binding;

use super::model::{MenuItem, WindowMenuOp};

/// Predefined menu item ids for the standard window operations. Any binding
/// (Rust / Node / React) that gives a menu item one of these ids gets the
/// built-in behavior for free — no callback wiring required.
pub const WINDOW_CASCADE_ID: &str = "atto_ui:window_cascade";
pub const WINDOW_TILE_ID: &str = "atto_ui:window_tile";
pub const WINDOW_MINIMIZE_ID: &str = "atto_ui:window_minimize";
pub const WINDOW_MAXIMIZE_ID: &str = "atto_ui:window_maximize";
pub const WINDOW_RESTORE_ID: &str = "atto_ui:window_restore";
pub const WINDOW_CLOSE_ID: &str = "atto_ui:window_close";
pub const WINDOW_NEXT_ID: &str = "atto_ui:window_next";
pub const WINDOW_PREVIOUS_ID: &str = "atto_ui:window_previous";
pub const WINDOW_MINIMIZE_ALL_ID: &str = "atto_ui:window_minimize_all";
pub const WINDOW_RESTORE_ALL_ID: &str = "atto_ui:window_restore_all";
pub const WINDOW_CLOSE_ALL_ID: &str = "atto_ui:window_close_all";

/// The canonical predefined id for a window operation.
pub fn window_menu_op_id(op: WindowMenuOp) -> &'static str {
    match op {
        WindowMenuOp::Cascade => WINDOW_CASCADE_ID,
        WindowMenuOp::Tile => WINDOW_TILE_ID,
        WindowMenuOp::MinimizeFocused => WINDOW_MINIMIZE_ID,
        WindowMenuOp::MaximizeFocused => WINDOW_MAXIMIZE_ID,
        WindowMenuOp::RestoreFocused => WINDOW_RESTORE_ID,
        WindowMenuOp::CloseFocused => WINDOW_CLOSE_ID,
        WindowMenuOp::FocusNext => WINDOW_NEXT_ID,
        WindowMenuOp::FocusPrevious => WINDOW_PREVIOUS_ID,
        WindowMenuOp::MinimizeAll => WINDOW_MINIMIZE_ALL_ID,
        WindowMenuOp::RestoreAll => WINDOW_RESTORE_ALL_ID,
        WindowMenuOp::CloseAll => WINDOW_CLOSE_ALL_ID,
    }
}

/// Resolve a predefined id string to its window operation, if it is one.
pub fn window_menu_op_from_id(id: &str) -> Option<WindowMenuOp> {
    let op = match id {
        WINDOW_CASCADE_ID => WindowMenuOp::Cascade,
        WINDOW_TILE_ID => WindowMenuOp::Tile,
        WINDOW_MINIMIZE_ID => WindowMenuOp::MinimizeFocused,
        WINDOW_MAXIMIZE_ID => WindowMenuOp::MaximizeFocused,
        WINDOW_RESTORE_ID => WindowMenuOp::RestoreFocused,
        WINDOW_CLOSE_ID => WindowMenuOp::CloseFocused,
        WINDOW_NEXT_ID => WindowMenuOp::FocusNext,
        WINDOW_PREVIOUS_ID => WindowMenuOp::FocusPrevious,
        WINDOW_MINIMIZE_ALL_ID => WindowMenuOp::MinimizeAll,
        WINDOW_RESTORE_ALL_ID => WindowMenuOp::RestoreAll,
        WINDOW_CLOSE_ALL_ID => WindowMenuOp::CloseAll,
        _ => return None,
    };
    Some(op)
}

pub(super) fn window_menu_op(item: &MenuItem) -> Option<WindowMenuOp> {
    window_menu_op_from_id(item.tag.as_deref()?)
}

impl MenuItem {
    /// Build a leaf menu item bound to a standard window operation. Activating it
    /// performs the operation natively; no callback is needed.
    pub fn window_op(op: WindowMenuOp, label: impl Into<Binding<String>>) -> Self {
        let mut item = Self::submenu(label, Vec::new());
        item.tag = Some(window_menu_op_id(op).to_string());
        item
    }
}
