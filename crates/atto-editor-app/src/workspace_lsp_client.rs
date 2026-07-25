//! `EditorView` LSP client backed by the app-wide workspace LSP bridge.

use atto_ui_editor::EditorLspClient;
use editor_core::{BufferId, EditorStateManager, LineIndex, TextDelta};
use editor_core_lsp::{LspContentChange, LspEvent, SemanticTokensLegend};
use serde_json::Value;

use crate::workspace_state::SharedWorkspaceState;

pub struct WorkspaceEditorLspClient {
    workspace: SharedWorkspaceState,
    buffer_id: BufferId,
    uri: String,
}

impl WorkspaceEditorLspClient {
    pub fn new(workspace: SharedWorkspaceState, buffer_id: BufferId, uri: String) -> Self {
        Self {
            workspace,
            buffer_id,
            uri,
        }
    }

    fn with_session_mut<T>(
        &self,
        f: impl FnOnce(&mut editor_core_lsp::LspSession) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut workspace = self.workspace.lock();
        let session = workspace.lsp.session_mut_for_buffer(self.buffer_id)?;
        f(session)
    }
}

impl EditorLspClient for WorkspaceEditorLspClient {
    fn uri(&self) -> Option<String> {
        let workspace = self.workspace.lock();
        workspace
            .lsp
            .document_uri(&workspace.workspace, self.buffer_id)
            .or_else(|| Some(self.uri.clone()))
    }

    fn poll(&mut self, _state_manager: &mut EditorStateManager) -> Result<Vec<LspEvent>, String> {
        Ok(self
            .workspace
            .lock()
            .lsp
            .drain_document_events(self.buffer_id))
    }

    fn drain_events(&mut self) -> Vec<LspEvent> {
        self.workspace
            .lock()
            .lsp
            .drain_document_events(self.buffer_id)
    }

    fn supports_semantic_tokens(&self) -> bool {
        self.workspace
            .lock()
            .lsp
            .supports_semantic_tokens(self.buffer_id)
    }

    fn supports_semantic_tokens_delta(&self) -> bool {
        self.workspace
            .lock()
            .lsp
            .supports_semantic_tokens_delta(self.buffer_id)
    }

    fn supports_folding_ranges(&self) -> bool {
        self.workspace
            .lock()
            .lsp
            .supports_folding_ranges(self.buffer_id)
    }

    fn semantic_legend(&self) -> Option<SemanticTokensLegend> {
        self.workspace.lock().lsp.semantic_legend(self.buffer_id)
    }

    fn did_change(&mut self, _uri: &str, change: LspContentChange) -> Result<(), String> {
        self.workspace
            .lock()
            .sync_buffer_text(self.buffer_id, &change.text)?;
        Ok(())
    }

    fn did_change_from_delta(&mut self, _uri: &str, delta: &TextDelta) -> Result<(), String> {
        let mut workspace = self.workspace.lock();
        let before = workspace.buffer_text(self.buffer_id)?;
        let Some(after) = apply_text_delta(&before, delta) else {
            return Ok(());
        };
        workspace.sync_buffer_text(self.buffer_id, &after)?;
        Ok(())
    }

    fn request_hover(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_hover_for_uri(uri, line_index, line, column)
        })
    }

    fn request_completion(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_completion_for_uri(uri, line_index, line, column)
        })
    }

    fn request_signature_help(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_signature_help_for_uri(uri, line_index, line, column)
        })
    }

    fn request_definition(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_definition_for_uri(uri, line_index, line, column)
        })
    }

    fn request_declaration(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_declaration_for_uri(uri, line_index, line, column)
        })
    }

    fn request_type_definition(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_type_definition_for_uri(uri, line_index, line, column)
        })
    }

    fn request_implementation(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_implementation_for_uri(uri, line_index, line, column)
        })
    }

    fn request_references(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
        include_declaration: bool,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_references_for_uri(uri, line_index, line, column, include_declaration)
        })
    }

    fn request_code_action(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        start_offset: usize,
        end_offset: usize,
        context: Value,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_code_action_for_uri(uri, line_index, start_offset, end_offset, context)
        })
    }

    fn request_formatting(&mut self, uri: &str, options: Value) -> Result<u64, String> {
        self.with_session_mut(|session| session.request_formatting_for_uri(uri, options))
    }

    fn request_prepare_rename(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_prepare_rename_for_uri(uri, line_index, line, column)
        })
    }

    fn request_rename(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
        new_name: String,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_rename_for_uri(uri, line_index, line, column, new_name)
        })
    }

    fn request_document_symbols(&mut self, uri: &str) -> Result<u64, String> {
        self.with_session_mut(|session| session.request_document_symbols_for_uri(uri))
    }

    fn request_workspace_symbol(&mut self, query: String) -> Result<u64, String> {
        self.workspace
            .lock()
            .lsp
            .request_workspace_symbols_for_buffer(self.buffer_id, query)
    }

    fn request_inlay_hints(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_inlay_hints_for_uri(uri, line_index, start_offset, end_offset)
        })
    }

    fn request_semantic_tokens_full(&mut self, uri: &str) -> Result<u64, String> {
        self.with_session_mut(|session| session.request_semantic_tokens_full_for_uri(uri))
    }

    fn request_semantic_tokens_delta(
        &mut self,
        uri: &str,
        previous_result_id: Option<String>,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| {
            session.request_semantic_tokens_delta_for_uri(uri, previous_result_id)
        })
    }

    fn request_folding_ranges(&mut self, uri: &str) -> Result<u64, String> {
        self.with_session_mut(|session| session.request_folding_ranges_for_uri(uri))
    }

    fn request_execute_command(
        &mut self,
        command: String,
        arguments: Vec<Value>,
    ) -> Result<u64, String> {
        self.with_session_mut(|session| session.request_execute_command(command, arguments))
    }
}

fn apply_text_delta(text: &str, delta: &TextDelta) -> Option<String> {
    let mut chars = text.chars().collect::<Vec<_>>();
    if chars.len() != delta.before_char_count {
        return None;
    }

    for edit in &delta.edits {
        let start = edit.start;
        let end = edit.end();
        if end > chars.len() {
            return None;
        }

        let deleted = chars[start..end].iter().collect::<String>();
        if deleted != edit.deleted_text {
            return None;
        }

        chars.splice(start..end, edit.inserted_text.chars());
    }

    if chars.len() != delta.after_char_count {
        return None;
    }
    Some(chars.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::TextDeltaEdit;

    #[test]
    fn apply_text_delta_uses_sequential_char_offsets() {
        let delta = TextDelta {
            before_char_count: 4,
            after_char_count: 4,
            edits: vec![
                TextDeltaEdit {
                    start: 1,
                    deleted_text: "👋".to_string(),
                    inserted_text: "X".to_string(),
                },
                TextDeltaEdit {
                    start: 3,
                    deleted_text: "c".to_string(),
                    inserted_text: "Y".to_string(),
                },
            ],
            undo_group_id: None,
        };

        assert_eq!(apply_text_delta("a👋bc", &delta), Some("aXbY".to_string()));
    }
}
