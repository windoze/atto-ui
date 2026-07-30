//! Document-symbol picker: symbol request, results picker, and item building.

use super::super::*;

pub(crate) fn process_document_symbol_picker_events(
    desktop: &mut Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().document_symbol_picker_events.clone();
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_document_symbol_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(_) => restore_document_symbol_picker_focus(desktop, state),
            PickerEvent::Closed => restore_document_symbol_picker_focus(desktop, state),
        }
    }
}

pub(crate) fn restore_document_symbol_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.document_symbol_picker_window = None;
        s.document_symbol_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

pub(crate) fn request_document_symbols(desktop: &Desktop, state: &Arc<Mutex<AppState>>) {
    if let Some(cmds) = active_editor_commands(desktop, state) {
        set_status_message(state, "Requesting document symbols…");
        cmds.push(EditorWindowCommand::RequestDocumentSymbols);
    } else {
        set_status_message(state, "No active editor for document symbols");
    }
}

pub(crate) fn open_document_symbol_results_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    outline: DocumentOutline,
) {
    if let Some(id) = state.lock().document_symbol_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().document_symbol_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.document_symbol_picker_events.clone();
        let _ = events.drain();
        s.document_symbol_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new(
        "Document Symbols",
        document_symbol_items(&outline),
        events.clone(),
    )
    .placeholder("Type a document symbol")
    .max_results(MAX_SYMBOL_PICKER_RESULTS)
    .border(false);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 20);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Document Symbols", rect, Box::new(view))
            .with_tag("atto-editor-app-document-symbols")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.document_symbol_picker_window == Some(id) {
                        s.document_symbol_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().document_symbol_picker_window = Some(id);
}

pub(crate) fn document_symbol_items(outline: &DocumentOutline) -> Vec<PickerItem<AppAction>> {
    let mut items = Vec::new();
    for symbol in &outline.symbols {
        push_document_symbol_item(&mut items, symbol, 0);
    }
    items
}

pub(crate) fn push_document_symbol_item(
    items: &mut Vec<PickerItem<AppAction>>,
    symbol: &DocumentSymbol,
    depth: usize,
) {
    let title = format!("{}{}", "  ".repeat(depth), symbol.name);
    let mut subtitle = symbol_kind_label(symbol.kind).to_string();
    if let Some(detail) = &symbol.detail
        && !detail.is_empty()
    {
        subtitle.push_str(" · ");
        subtitle.push_str(detail);
    }
    items.push(
        PickerItem::new(
            title,
            AppAction::JumpTo(JumpTarget::CharOffset {
                offset: symbol.selection_range.start,
            }),
        )
        .subtitle(subtitle),
    );
    for child in &symbol.children {
        push_document_symbol_item(items, child, depth + 1);
    }
}
