//! Command-palette picker: event processing, focus restore, open, and item list.

use super::super::*;

pub(crate) fn process_command_palette_events(
    desktop: &mut Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().command_palette_events.clone();
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_command_palette_focus(desktop, state);
                execute_command_action(desktop, state, actions, action);
            }
            PickerEvent::Submitted(_) => restore_command_palette_focus(desktop, state),
            PickerEvent::Closed => restore_command_palette_focus(desktop, state),
        }
    }
}

pub(crate) fn restore_command_palette_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.command_palette_window = None;
        s.command_palette_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

pub(crate) fn open_command_palette(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
) {
    if let Some(id) = state.lock().command_palette_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().command_palette_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.command_palette_events.clone();
        let _ = events.drain();
        s.command_palette_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new("Command Palette", command_palette_items(), events.clone())
        .placeholder("Type a command")
        .max_results(200)
        .border(false);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 76, 18);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Command Palette", rect, Box::new(view))
            .with_tag("atto-editor-app-command-palette")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.command_palette_window == Some(id) {
                        s.command_palette_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().command_palette_window = Some(id);
}

pub(crate) fn command_palette_items() -> Vec<PickerItem<AppCommandAction>> {
    let registry = commands::app_command_registry();
    registry
        .commands()
        .iter()
        .map(|command| {
            let mut item = PickerItem::new(command.title.clone(), command.action.clone())
                .subtitle(command.category.clone());
            if let Some(sequence) = &command.default_sequence {
                item = item.shortcut(sequence.label());
            }
            item
        })
        .collect()
}
