//! Workspace-symbol picker: symbol query request, results picker, item
//! building, and subtitle formatting.

use super::super::*;

pub(crate) fn process_workspace_symbol_picker_events(
    desktop: &mut Desktop,
    _screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().workspace_symbol_picker_events.clone();
    let mut submitted_query = None;
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_workspace_symbol_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(query) => {
                restore_workspace_symbol_picker_focus(desktop, state);
                submitted_query = Some(query);
            }
            PickerEvent::Closed => restore_workspace_symbol_picker_focus(desktop, state),
        }
    }
    if let Some(query) = submitted_query {
        request_workspace_symbols(desktop, state, query);
    }
}

pub(crate) fn restore_workspace_symbol_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.workspace_symbol_picker_window = None;
        s.workspace_symbol_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

pub(crate) fn open_workspace_symbol_query_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
) {
    if let Some(id) = state.lock().workspace_symbol_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().workspace_symbol_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.workspace_symbol_picker_events.clone();
        let _ = events.drain();
        s.workspace_symbol_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::<AppAction>::new("Workspace Symbols", Vec::new(), events.clone())
        .placeholder("Type a symbol query and press Enter")
        .submit_query_on_empty(true);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 12);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Workspace Symbols", rect, Box::new(view))
            .with_tag("atto-editor-app-workspace-symbols")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.workspace_symbol_picker_window == Some(id) {
                        s.workspace_symbol_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().workspace_symbol_picker_window = Some(id);
}

pub(crate) fn request_workspace_symbols(
    _desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
    query: impl Into<String>,
) {
    let query = query.into();
    if query.trim().is_empty() {
        set_status_message(state, "Workspace symbol query is empty");
        return;
    }

    let workspace_state = state.lock().workspace_state.clone();
    let request = workspace_state
        .lock()
        .request_workspace_symbols(query.clone());
    match request {
        Ok(true) => set_status_message(
            state,
            format!("Requesting workspace symbols for “{query}”…"),
        ),
        Ok(false) => {
            set_status_message(
                state,
                "Workspace symbols require an active workspace LSP session",
            );
        }
        Err(err) => set_status_message(state, format!("Workspace symbol request failed: {err}")),
    }
}

pub(crate) fn open_workspace_symbol_results_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    symbols: Vec<WorkspaceSymbol>,
) {
    if let Some(id) = state.lock().workspace_symbol_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().workspace_symbol_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.workspace_symbol_picker_events.clone();
        let _ = events.drain();
        s.workspace_symbol_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new(
        "Workspace Symbols",
        workspace_symbol_items(&symbols),
        events.clone(),
    )
    .placeholder("Type to filter workspace symbols")
    .max_results(MAX_SYMBOL_PICKER_RESULTS)
    .border(false);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 88, 20);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Workspace Symbols", rect, Box::new(view))
            .with_tag("atto-editor-app-workspace-symbols")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.workspace_symbol_picker_window == Some(id) {
                        s.workspace_symbol_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().workspace_symbol_picker_window = Some(id);
}

pub(crate) fn workspace_symbol_items(symbols: &[WorkspaceSymbol]) -> Vec<PickerItem<AppAction>> {
    symbols
        .iter()
        .map(|symbol| {
            let action = match editor_core_lsp::file_uri_to_path(&symbol.location.uri) {
                Some(path) => AppAction::OpenPathAndJump {
                    path,
                    target: JumpTarget::Utf16Position {
                        line: symbol.location.range.start.line,
                        character: symbol.location.range.start.character,
                    },
                },
                None => AppAction::ShowStatusMessage(format!(
                    "Unsupported workspace symbol URI: {}",
                    symbol.location.uri
                )),
            };
            let subtitle = workspace_symbol_subtitle(symbol);
            PickerItem::new(symbol.name.clone(), action).subtitle(subtitle)
        })
        .collect()
}

pub(crate) fn workspace_symbol_subtitle(symbol: &WorkspaceSymbol) -> String {
    let mut parts = vec![symbol_kind_label(symbol.kind).to_string()];
    if let Some(container) = &symbol.container_name
        && !container.is_empty()
    {
        parts.push(container.clone());
    }
    if let Some(detail) = &symbol.detail
        && !detail.is_empty()
    {
        parts.push(detail.clone());
    }
    parts.push(format!(
        "{}:{}:{}",
        symbol.location.uri,
        symbol.location.range.start.line.saturating_add(1),
        symbol.location.range.start.character.saturating_add(1)
    ));
    parts.join(" · ")
}
