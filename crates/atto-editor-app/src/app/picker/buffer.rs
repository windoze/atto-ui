//! Buffer picker: event processing, focus restore, open, and open-buffer items.

use super::super::*;

pub(crate) fn process_buffer_picker_events(
    desktop: &mut Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().buffer_picker_events.clone();
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_buffer_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(_) => restore_buffer_picker_focus(desktop, state),
            PickerEvent::Closed => restore_buffer_picker_focus(desktop, state),
        }
    }
}

pub(crate) fn restore_buffer_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.buffer_picker_window = None;
        s.buffer_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

pub(crate) fn open_buffer_picker(desktop: &mut Desktop, screen: Rect, state: &Arc<Mutex<AppState>>) {
    if let Some(id) = state.lock().buffer_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().buffer_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.buffer_picker_events.clone();
        let _ = events.drain();
        s.buffer_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new(
        "Buffer Picker",
        buffer_picker_items(desktop, state),
        events.clone(),
    )
    .placeholder("Type a buffer name")
    .max_results(200)
    .border(false);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 18);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Buffer Picker", rect, Box::new(view))
            .with_tag("atto-editor-app-buffer-picker")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.buffer_picker_window == Some(id) {
                        s.buffer_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().buffer_picker_window = Some(id);
}

pub(crate) fn buffer_picker_items(
    desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
) -> Vec<PickerItem<AppAction>> {
    let summaries = state.lock().editor_tab_summaries.clone();
    let mut items = Vec::new();
    for window in desktop.wm.windows() {
        let window_id = window.id();
        let Some(tab_summaries) = summaries.get(&window_id) else {
            continue;
        };
        for summary in tab_summaries.get() {
            items.push(buffer_picker_item(window_id, summary));
        }
    }
    items
}

pub(crate) fn buffer_picker_item(window: WindowId, summary: EditorTabSummary) -> PickerItem<AppAction> {
    let title = if summary.dirty {
        format!("{}*", summary.title)
    } else {
        summary.title
    };
    let path = summary
        .path
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "[Untitled]".to_string());
    let active = if summary.active { "Active" } else { "Open" };

    PickerItem::new(
        title,
        AppAction::SelectEditorTab {
            window,
            tab_id: summary.tab_id,
        },
    )
    .subtitle(format!("{active} · Window {} · {path}", window.raw()))
}
