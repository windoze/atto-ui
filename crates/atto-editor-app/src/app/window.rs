//! Shared editor-focus resolution ([`active_editor_commands`]) used across
//! dispatch, save-as, and event handlers.

use super::*;

pub(crate) fn active_editor_commands(
    desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
) -> Option<EventQueue<EditorWindowCommand>> {
    let focused = desktop.wm.focused();

    // We must be careful not to hold the `state` lock across an `if let` body and then try to
    // lock it again (that would deadlock). Grab the needed values in a single scoped lock.
    {
        let guard = state.lock();

        // Prefer the currently focused window if it's an editor.
        if let Some(id) = focused
            && let Some(cmds) = guard.editor_windows.get(&id)
        {
            return Some(cmds.clone());
        }

        // Fall back to the last-focused editor window (useful when Explorer is focused).
        if let Some(id) = guard.last_focused_editor
            && desktop.wm.window(id).is_some()
            && let Some(cmds) = guard.editor_windows.get(&id)
        {
            return Some(cmds.clone());
        }
    }

    // Finally, fall back to the topmost editor window in z-order.
    let guard = state.lock();
    for w in desktop.wm.windows().iter().rev() {
        if let Some(cmds) = guard.editor_windows.get(&w.id()) {
            return Some(cmds.clone());
        }
    }

    None
}
