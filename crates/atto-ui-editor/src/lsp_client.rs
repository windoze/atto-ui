//! LSP client abstraction used by `EditorView`.
//!
//! `EditorView` can run in two modes:
//! - standalone: the view owns a local `editor_core_lsp::LspSession`
//! - workspace-backed: the app injects a handle to a shared workspace session

use std::sync::{Arc, Mutex};

use editor_core::{EditorStateManager, LineIndex, TextDelta};
use editor_core_lsp::{
    LspContentChange, LspEvent, SemanticTokensLegend, SemanticTokensUpdate,
    folding_ranges_result_to_processing_edit, semantic_tokens_result_to_update,
};
use serde_json::Value;

/// LSP operations required by `EditorView`.
///
/// Document-scoped requests all take `uri` so a single backend session can serve multiple editor
/// views. Implementations that own a single-document session should still honor the URI when
/// possible; the local implementation delegates to `editor-core-lsp`'s `*_for_uri` APIs.
pub trait EditorLspClient: Send {
    /// Current document URI for the view this client is bound to.
    fn uri(&self) -> Option<String>;

    /// Poll the backend and return queued LSP events relevant to this client.
    ///
    /// Local clients also apply LSP-derived processing edits (diagnostics, semantic tokens,
    /// folding) directly to `state_manager`. Shared clients typically only drain events routed by
    /// the app-level workspace bridge.
    fn poll(&mut self, state_manager: &mut EditorStateManager) -> Result<Vec<LspEvent>, String>;

    /// Drain queued events without polling the transport.
    fn drain_events(&mut self) -> Vec<LspEvent>;

    /// Whether `poll` already applies semantic-token / folding derived state.
    fn applies_derived_state_in_poll(&self) -> bool {
        false
    }

    /// Semantic-token feature support.
    fn supports_semantic_tokens(&self) -> bool {
        false
    }

    /// Semantic-token delta feature support.
    fn supports_semantic_tokens_delta(&self) -> bool {
        false
    }

    /// Folding-range feature support.
    fn supports_folding_ranges(&self) -> bool {
        false
    }

    /// Server semantic-token legend, if available.
    fn semantic_legend(&self) -> Option<SemanticTokensLegend> {
        None
    }

    /// Notify a document change.
    fn did_change(&mut self, uri: &str, change: LspContentChange) -> Result<(), String>;

    /// Notify a document change from an editor-core `TextDelta`.
    fn did_change_from_delta(&mut self, uri: &str, delta: &TextDelta) -> Result<(), String>;

    fn request_hover(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String>;

    fn request_completion(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String>;

    fn request_signature_help(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String>;

    fn request_definition(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String>;

    fn request_declaration(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String>;

    fn request_type_definition(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String>;

    fn request_implementation(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String>;

    fn request_references(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
        include_declaration: bool,
    ) -> Result<u64, String>;

    fn request_code_action(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        start_offset: usize,
        end_offset: usize,
        context: Value,
    ) -> Result<u64, String>;

    fn request_formatting(&mut self, uri: &str, options: Value) -> Result<u64, String>;

    fn request_prepare_rename(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String>;

    fn request_rename(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
        new_name: String,
    ) -> Result<u64, String>;

    fn request_document_symbols(&mut self, uri: &str) -> Result<u64, String>;

    fn request_workspace_symbol(&mut self, query: String) -> Result<u64, String>;

    fn request_inlay_hints(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<u64, String>;

    fn request_semantic_tokens_full(&mut self, uri: &str) -> Result<u64, String>;

    fn request_semantic_tokens_delta(
        &mut self,
        uri: &str,
        previous_result_id: Option<String>,
    ) -> Result<u64, String>;

    fn request_folding_ranges(&mut self, uri: &str) -> Result<u64, String>;

    fn request_execute_command(
        &mut self,
        command: String,
        arguments: Vec<Value>,
    ) -> Result<u64, String>;
}

/// Shared LSP client handle used by `EditorView`.
pub type SharedEditorLspClient = Arc<Mutex<dyn EditorLspClient + Send>>;

/// Wrap an LSP client in a shared handle.
pub fn shared_lsp_client(client: impl EditorLspClient + 'static) -> SharedEditorLspClient {
    Arc::new(Mutex::new(client))
}

/// Standalone LSP client backed by a single `LspSession`.
pub struct LocalLspClient {
    session: editor_core_lsp::LspSession,
}

impl LocalLspClient {
    /// Create a standalone client from an initialized session.
    pub fn new(session: editor_core_lsp::LspSession) -> Self {
        Self { session }
    }

    /// Get the underlying session.
    pub fn session(&self) -> &editor_core_lsp::LspSession {
        &self.session
    }

    /// Get the underlying session mutably.
    pub fn session_mut(&mut self) -> &mut editor_core_lsp::LspSession {
        &mut self.session
    }
}

impl EditorLspClient for LocalLspClient {
    fn uri(&self) -> Option<String> {
        Some(self.session.document().uri.clone())
    }

    fn poll(&mut self, state_manager: &mut EditorStateManager) -> Result<Vec<LspEvent>, String> {
        state_manager
            .apply_processor(&mut self.session)
            .map_err(|err| err.to_string())?;
        Ok(self.session.drain_events())
    }

    fn drain_events(&mut self) -> Vec<LspEvent> {
        self.session.drain_events()
    }

    fn applies_derived_state_in_poll(&self) -> bool {
        true
    }

    fn supports_semantic_tokens(&self) -> bool {
        self.session.supports_semantic_tokens()
    }

    fn supports_semantic_tokens_delta(&self) -> bool {
        self.session.supports_semantic_tokens_delta()
    }

    fn supports_folding_ranges(&self) -> bool {
        self.session.supports_folding_range()
    }

    fn semantic_legend(&self) -> Option<SemanticTokensLegend> {
        self.session.semantic_legend().cloned()
    }

    fn did_change(&mut self, uri: &str, change: LspContentChange) -> Result<(), String> {
        self.session.did_change_for_uri(uri, change)
    }

    fn did_change_from_delta(&mut self, _uri: &str, delta: &TextDelta) -> Result<(), String> {
        self.session.did_change_from_text_delta(delta)
    }

    fn request_hover(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.session
            .request_hover_for_uri(uri, line_index, line, column)
    }

    fn request_completion(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.session
            .request_completion_for_uri(uri, line_index, line, column)
    }

    fn request_signature_help(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.session
            .request_signature_help_for_uri(uri, line_index, line, column)
    }

    fn request_definition(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.session
            .request_definition_for_uri(uri, line_index, line, column)
    }

    fn request_declaration(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.session
            .request_declaration_for_uri(uri, line_index, line, column)
    }

    fn request_type_definition(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.session
            .request_type_definition_for_uri(uri, line_index, line, column)
    }

    fn request_implementation(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.session
            .request_implementation_for_uri(uri, line_index, line, column)
    }

    fn request_references(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
        include_declaration: bool,
    ) -> Result<u64, String> {
        self.session
            .request_references_for_uri(uri, line_index, line, column, include_declaration)
    }

    fn request_code_action(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        start_offset: usize,
        end_offset: usize,
        context: Value,
    ) -> Result<u64, String> {
        self.session
            .request_code_action_for_uri(uri, line_index, start_offset, end_offset, context)
    }

    fn request_formatting(&mut self, uri: &str, options: Value) -> Result<u64, String> {
        self.session.request_formatting_for_uri(uri, options)
    }

    fn request_prepare_rename(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
    ) -> Result<u64, String> {
        self.session
            .request_prepare_rename_for_uri(uri, line_index, line, column)
    }

    fn request_rename(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        line: usize,
        column: usize,
        new_name: String,
    ) -> Result<u64, String> {
        self.session
            .request_rename_for_uri(uri, line_index, line, column, new_name)
    }

    fn request_document_symbols(&mut self, uri: &str) -> Result<u64, String> {
        self.session.request_document_symbols_for_uri(uri)
    }

    fn request_workspace_symbol(&mut self, query: String) -> Result<u64, String> {
        self.session.request_workspace_symbol(query)
    }

    fn request_inlay_hints(
        &mut self,
        uri: &str,
        line_index: &LineIndex,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<u64, String> {
        self.session
            .request_inlay_hints_for_uri(uri, line_index, start_offset, end_offset)
    }

    fn request_semantic_tokens_full(&mut self, uri: &str) -> Result<u64, String> {
        self.session.request_semantic_tokens_full_for_uri(uri)
    }

    fn request_semantic_tokens_delta(
        &mut self,
        uri: &str,
        previous_result_id: Option<String>,
    ) -> Result<u64, String> {
        self.session
            .request_semantic_tokens_delta_for_uri(uri, previous_result_id)
    }

    fn request_folding_ranges(&mut self, uri: &str) -> Result<u64, String> {
        self.session.request_folding_ranges_for_uri(uri)
    }

    fn request_execute_command(
        &mut self,
        command: String,
        arguments: Vec<Value>,
    ) -> Result<u64, String> {
        self.session.request_execute_command(command, arguments)
    }
}

/// Apply a semantic-token response using caller-owned per-document baseline state.
pub fn semantic_tokens_update_for_view(
    result: &Value,
    baseline: &[u32],
    legend: Option<&SemanticTokensLegend>,
    line_index: &LineIndex,
) -> Option<SemanticTokensUpdate> {
    semantic_tokens_result_to_update(result, baseline, legend, line_index)
}

/// Convert a folding-range response into a processing edit for a view.
pub fn folding_ranges_edit_for_view(result: &Value) -> editor_core::ProcessingEdit {
    folding_ranges_result_to_processing_edit(result)
}
