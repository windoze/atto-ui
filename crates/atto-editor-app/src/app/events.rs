//! Event-loop fan-out: workspace-LSP and editor-event processing, rename
//! workspace-edit application, and symbol-kind labeling.

use super::*;

pub(crate) fn process_workspace_lsp_events(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
) {
    let workspace_state = state.lock().workspace_state.clone();
    let (events, last_error) = {
        let mut workspace = workspace_state.lock();
        let events = workspace.poll_lsp();
        let last_error = workspace.take_last_error();
        (events, last_error)
    };

    if let Some(error) = last_error {
        set_status_message(state, error);
    }

    for event in events {
        match event {
            LspWorkspaceEvent::WorkspaceSymbols { query, symbols } => {
                let count = symbols.len();
                open_workspace_symbol_results_picker(desktop, screen, state, symbols);
                set_status_message(
                    state,
                    format!("Workspace symbols for “{query}”: {count} result(s)"),
                );
            }
            LspWorkspaceEvent::WorkspaceEditApplied { result } => {
                let applied = result.applied.len();
                if result.skipped_uris.is_empty() {
                    set_status_message(
                        state,
                        format!("Workspace edit applied to {applied} file(s)"),
                    );
                } else {
                    set_status_message(
                        state,
                        format!(
                            "Workspace edit applied to {applied} file(s), skipped {} unopened file(s)",
                            result.skipped_uris.len()
                        ),
                    );
                }
            }
            LspWorkspaceEvent::Message(message) => set_status_message(state, message),
        }
    }
}

pub(crate) fn process_editor_events(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
) {
    let queues = {
        let s = state.lock();
        s.editor_events
            .iter()
            .map(|(id, events)| (*id, events.clone()))
            .collect::<Vec<_>>()
    };

    for (window, events) in queues {
        if desktop.wm.window(window).is_none() {
            continue;
        }
        for event in events.drain() {
            match event {
                atto_ui_editor::EditorEvent::LspGoto { .. } => {}
                atto_ui_editor::EditorEvent::DocumentSymbols { outline } => {
                    open_document_symbol_results_picker(desktop, screen, state, outline);
                }
                atto_ui_editor::EditorEvent::WorkspaceSymbols { query: _, symbols } => {
                    open_workspace_symbol_results_picker(desktop, screen, state, symbols);
                }
                atto_ui_editor::EditorEvent::CodeActionMessage { message } => {
                    set_status_message(state, message);
                }
                atto_ui_editor::EditorEvent::LspRenameWorkspaceEdit { edit } => {
                    apply_rename_workspace_edit(state, edit);
                }
                atto_ui_editor::EditorEvent::LspMessage { message } => {
                    set_status_message(state, message);
                }
                atto_ui_editor::EditorEvent::FormatFinished { success, changed } => {
                    if success {
                        if changed {
                            set_status_message(state, "Formatted document");
                        } else {
                            set_status_message(state, "Format produced no changes");
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn apply_rename_workspace_edit(state: &Arc<Mutex<AppState>>, edit: serde_json::Value) {
    let workspace_state = state.lock().workspace_state.clone();
    let result = {
        let mut workspace = workspace_state.lock();
        if workspace.active_buffer_id().is_none() {
            set_status_message(state, "Rename requires workspace support");
            return;
        }
        workspace.apply_workspace_edit(&edit)
    };

    match result {
        Ok(result) => {
            let applied = result.applied.len();
            let skipped = result.skipped_uris.len();
            match (applied, skipped) {
                (0, 0) => set_status_message(state, "Rename produced no changes"),
                (0, _) => set_status_message(
                    state,
                    format!("Rename skipped {skipped} unopened file(s); no open files updated"),
                ),
                (_, 0) => {
                    set_status_message(state, format!("Rename applied to {applied} file(s)"));
                }
                (_, _) => set_status_message(
                    state,
                    format!(
                        "Rename applied to {applied} file(s), skipped {skipped} unopened file(s)"
                    ),
                ),
            }
        }
        Err(err) => set_status_message(state, format!("Rename failed: {err}")),
    }
}

pub(crate) fn symbol_kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::File => "File",
        SymbolKind::Module => "Module",
        SymbolKind::Namespace => "Namespace",
        SymbolKind::Package => "Package",
        SymbolKind::Class => "Class",
        SymbolKind::Method => "Method",
        SymbolKind::Property => "Property",
        SymbolKind::Field => "Field",
        SymbolKind::Constructor => "Constructor",
        SymbolKind::Enum => "Enum",
        SymbolKind::Interface => "Interface",
        SymbolKind::Function => "Function",
        SymbolKind::Variable => "Variable",
        SymbolKind::Constant => "Constant",
        SymbolKind::String => "String",
        SymbolKind::Number => "Number",
        SymbolKind::Boolean => "Boolean",
        SymbolKind::Array => "Array",
        SymbolKind::Object => "Object",
        SymbolKind::Key => "Key",
        SymbolKind::Null => "Null",
        SymbolKind::EnumMember => "EnumMember",
        SymbolKind::Struct => "Struct",
        SymbolKind::Event => "Event",
        SymbolKind::Operator => "Operator",
        SymbolKind::TypeParameter => "TypeParameter",
        SymbolKind::Custom(_) => "Custom",
    }
}
