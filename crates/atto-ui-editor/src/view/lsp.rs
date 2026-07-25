// LSP integration + hover/completion popup plumbing.

use super::*;
use crate::lsp_client::{LocalLspClient, shared_lsp_client};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

impl EditorView {
    /// Inject an externally managed LSP client.
    ///
    /// This is used by `atto-editor-app` to bind the view to the workspace-shared
    /// `(workspace_root, language)` LSP session instead of starting a per-view server.
    pub fn set_lsp_client(&mut self, client: SharedEditorLspClient) {
        self.lsp.client = Some(client);
        self.reset_inlay_hint_tracking();
        self.reset_shared_derived_state();
        editor_core_lsp::clear_lsp_state(&mut self.state_manager);
        self.clear_lsp_diagnostics();
        self.maybe_apply_syntax_highlighting();
    }

    pub(super) fn start_lsp_if_enabled(&mut self) {
        if let EditorLspMode::Enabled(cfg) = self.config.lsp.get() {
            self.start_lsp_session(cfg);
        }
    }

    pub(super) fn lsp_did_change(&mut self, change: LspContentChange) {
        let Some(uri) = self.current_lsp_uri() else {
            return;
        };
        let Some(client) = self.lsp.client.clone() else {
            return;
        };
        let result = client
            .lock()
            .expect("LSP client mutex poisoned")
            .did_change(&uri, change);
        self.handle_did_change_result(result);
    }

    /// Forward the structured `TextDelta` an edit produced to the LSP server. Preferred over
    /// `lsp_did_change`: the delta carries the exact edit, so the server mirror never desyncs on
    /// multi-char edits (auto-pair deletion, re-indentation, multi-cursor). See
    /// `execute_edit_and_sync_delta`.
    pub(super) fn lsp_did_change_from_delta(&mut self, delta: &editor_core::TextDelta) {
        let Some(uri) = self.current_lsp_uri() else {
            return;
        };
        let Some(client) = self.lsp.client.clone() else {
            return;
        };
        let result = client
            .lock()
            .expect("LSP client mutex poisoned")
            .did_change_from_delta(&uri, delta);
        self.handle_did_change_result(result);
    }

    fn handle_did_change_result(&mut self, result: Result<(), String>) {
        if result.is_err() {
            self.lsp.client = None;
            self.reset_inlay_hint_tracking();
            self.reset_shared_derived_state();
            editor_core_lsp::clear_lsp_state(&mut self.state_manager);
            self.clear_lsp_diagnostics();
            self.maybe_apply_syntax_highlighting();
        }
    }

    pub(super) fn maybe_poll_lsp(&mut self) {
        let Some(client) = self.lsp.client.clone() else {
            return;
        };

        let poll_result = client
            .lock()
            .expect("LSP client mutex poisoned")
            .poll(&mut self.state_manager);

        let events = match poll_result {
            Ok(events) => events,
            Err(err) => {
                self.finish_pending_formatting_failure(format!(
                    "Formatting failed: LSP session error: {err}"
                ));
                self.lsp.client = None;
                self.reset_inlay_hint_tracking();
                self.reset_shared_derived_state();
                editor_core_lsp::clear_lsp_state(&mut self.state_manager);
                self.clear_lsp_diagnostics();
                self.maybe_apply_syntax_highlighting();
                return;
            }
        };

        for ev in events {
            match ev {
                editor_core_lsp::LspEvent::Notification(
                    editor_core_lsp::LspNotification::PublishDiagnostics(params),
                ) => self.apply_publish_diagnostics(params),
                editor_core_lsp::LspEvent::Notification(_) => {}
                editor_core_lsp::LspEvent::DeferredRequest(_) => {}
                editor_core_lsp::LspEvent::Response(resp) => self.handle_lsp_response(resp),
            }
        }
        self.maybe_request_shared_derived_state();
        self.maybe_timeout_pending_formatting();
    }

    pub(super) fn handle_lsp_response(&mut self, resp: editor_core_lsp::LspResponse) {
        if let Some(response_uri) = resp.uri.as_deref()
            && self.current_lsp_uri().as_deref() != Some(response_uri)
        {
            return;
        }

        let method = resp.method;
        let id = resp.id;
        let result = resp.result;
        let error = resp.error;

        if let Some((pending_id, kind)) = self.lsp.pending_goto
            && pending_id == id
        {
            let locs = result
                .as_ref()
                .map(locations_from_value)
                .unwrap_or_default();
            self.events.push(EditorEvent::LspGoto {
                kind,
                locations: locs,
            });
            self.lsp.pending_goto = None;
        }

        if let Some(pending_id) = self.lsp.pending_document_symbols
            && pending_id == id
            && method.as_str() == "textDocument/documentSymbol"
        {
            self.lsp.pending_document_symbols = None;
            let outline = result
                .as_ref()
                .map(|value| {
                    editor_core_lsp::lsp_document_symbols_to_outline(
                        self.state_manager.editor().line_index(),
                        value,
                    )
                })
                .unwrap_or_default();
            self.events.push(EditorEvent::DocumentSymbols { outline });
        }

        if let Some((pending_id, query)) = self.lsp.pending_workspace_symbols.clone()
            && pending_id == id
            && method.as_str() == "workspace/symbol"
        {
            self.lsp.pending_workspace_symbols = None;
            let symbols = result
                .as_ref()
                .map(editor_core_lsp::lsp_workspace_symbols_to_results)
                .unwrap_or_default();
            self.events
                .push(EditorEvent::WorkspaceSymbols { query, symbols });
        }

        if self.handle_shared_derived_response(id, &method, result.as_ref(), error.as_ref()) {
            return;
        }

        if let Some(pending_id) = self.lsp.hover_pending_request
            && pending_id == id
            && method.as_str() == "textDocument/hover"
        {
            self.lsp.hover_pending_request = None;
            let requested = self.lsp.hover_requested.take();
            if error.is_some() {
                self.hover_popup.set(None);
                return;
            }

            let Some(result) = result.as_ref() else {
                self.hover_popup.set(None);
                return;
            };

            let anchor = requested.or(self.lsp.hover_anchor).or_else(|| {
                self.cursor_screen_position()
                    .and_then(|p| p)
                    .map(|p| HoverAnchor {
                        position: self.active_cursor_position(),
                        screen: p,
                    })
            });

            if let Some(anchor) = anchor {
                self.handle_lsp_hover_response(result, anchor);
            } else {
                self.hover_popup.set(None);
            }
        }

        if let Some(pending_id) = self.lsp.completion_pending_request
            && pending_id == id
            && method.as_str() == "textDocument/completion"
        {
            self.lsp.completion_pending_request = None;
            let requested_position = self.lsp.completion_requested_position.take();
            if requested_position.is_some_and(|pos| pos != self.active_cursor_position()) {
                return;
            }
            if error.is_some() {
                self.completion_popup.set(None);
                return;
            }
            if let Some(result) = result.as_ref() {
                self.handle_lsp_completion_response(result);
            } else {
                self.completion_popup.set(None);
            }
        }

        if let Some(pending_id) = self.lsp.pending_signature_help
            && pending_id == id
            && method.as_str() == "textDocument/signatureHelp"
        {
            self.lsp.pending_signature_help = None;
            let requested_position = self.lsp.signature_help_requested_position.take();
            if error.is_some() {
                self.signature_help_popup.set(None);
                return;
            }
            if requested_position.is_some_and(|pos| pos != self.active_cursor_position()) {
                self.signature_help_popup.set(None);
                return;
            }
            if let Some(result) = result.as_ref() {
                self.handle_lsp_signature_help_response(result);
            } else {
                self.signature_help_popup.set(None);
            }
        }

        if let Some(pending_id) = self.lsp.pending_inlay_hints
            && pending_id == id
            && method.as_str() == "textDocument/inlayHint"
        {
            self.lsp.pending_inlay_hints = None;
            let pending_key = self.lsp.pending_inlay_key.take();
            let current_key = self.current_inlay_request_key();

            if pending_key != current_key {
                return;
            }

            if let Some(err) = error.as_ref() {
                self.events.push(EditorEvent::LspMessage {
                    message: format!("Inlay hints failed: {}", err.message),
                });
                return;
            }

            let result = result.as_ref().unwrap_or(&serde_json::Value::Null);
            self.apply_lsp_inlay_hints_response(result);
        }

        if let Some(pending_id) = self.lsp.pending_code_action
            && pending_id == id
            && method.as_str() == "textDocument/codeAction"
        {
            self.lsp.pending_code_action = None;
            if error.is_some() {
                self.lsp.code_action_items.clear();
                self.code_action_popup.set(None);
                return;
            }

            if let Some(result) = result.as_ref() {
                self.handle_lsp_code_action_response(result);
            } else {
                self.lsp.code_action_items.clear();
                self.code_action_popup.set(None);
            }
        }

        if let Some(pending) = self.lsp.pending_formatting
            && pending.id == id
            && method.as_str() == "textDocument/formatting"
        {
            self.lsp.pending_formatting = None;
            if let Some(err) = error.as_ref() {
                self.events.push(EditorEvent::LspMessage {
                    message: format!("Formatting failed: {}", err.message),
                });
                self.events.push(EditorEvent::FormatFinished {
                    success: false,
                    changed: false,
                });
                return;
            }

            let Some(result) = result.as_ref().filter(|value| !value.is_null()) else {
                self.events.push(EditorEvent::FormatFinished {
                    success: true,
                    changed: false,
                });
                return;
            };

            self.handle_lsp_formatting_response(result);
        }

        if let Some((pending_id, target)) = self.lsp.pending_prepare_rename
            && pending_id == id
            && method.as_str() == "textDocument/prepareRename"
        {
            self.lsp.pending_prepare_rename = None;
            if let Some(err) = error {
                self.events.push(EditorEvent::LspMessage {
                    message: format!("Rename is not available: {}", err.message),
                });
                return;
            }

            let Some(result) = result.as_ref().filter(|value| !value.is_null()) else {
                self.events.push(EditorEvent::LspMessage {
                    message: "Rename is not available at the cursor".to_string(),
                });
                return;
            };

            let default_text = self
                .rename_default_from_prepare_response(result)
                .or_else(|| self.current_word_at_cursor())
                .unwrap_or_default();
            if default_text.is_empty() {
                self.events.push(EditorEvent::LspMessage {
                    message: "Rename is not available at the cursor".to_string(),
                });
                return;
            }

            self.open_rename_popup(target, default_text);
        }

        if let Some(pending_id) = self.lsp.pending_rename
            && pending_id == id
            && method.as_str() == "textDocument/rename"
        {
            self.lsp.pending_rename = None;
            self.lsp.rename_target = None;
            if let Some(err) = error {
                self.events.push(EditorEvent::LspMessage {
                    message: format!("Rename failed: {}", err.message),
                });
                return;
            }
            let Some(edit) = result.filter(|value| !value.is_null()) else {
                self.events.push(EditorEvent::LspMessage {
                    message: "Rename produced no workspace edit".to_string(),
                });
                return;
            };
            self.events
                .push(EditorEvent::LspRenameWorkspaceEdit { edit });
        }
    }

    fn apply_publish_diagnostics(&mut self, params: editor_core_lsp::LspPublishDiagnosticsParams) {
        if !self.publish_diagnostics_matches_current_document(&params) {
            return;
        }
        self.apply_current_document_diagnostics(params);
    }

    fn publish_diagnostics_matches_current_document(
        &self,
        params: &editor_core_lsp::LspPublishDiagnosticsParams,
    ) -> bool {
        self.current_lsp_uri().is_some_and(|uri| params.uri == uri)
    }

    pub(super) fn apply_current_document_diagnostics(
        &mut self,
        params: editor_core_lsp::LspPublishDiagnosticsParams,
    ) {
        let edits = editor_core_lsp::lsp_diagnostics_to_processing_edits(
            self.state_manager.editor().line_index(),
            &params,
        );
        self.state_manager.apply_processing_edits(edits);

        let diagnostics = params.diagnostics;
        let summary = DiagnosticsSummary::from_diagnostics(&diagnostics);
        self.lsp.diagnostics = diagnostics;
        self.lsp.diagnostic_cursor = None;
        self.lsp.diagnostics_revision = self.lsp.diagnostics_revision.saturating_add(1);
        self.set_diagnostics_summary(summary);
    }

    fn set_diagnostics_summary(&mut self, summary: DiagnosticsSummary) {
        if self.diagnostics_summary.get() != summary {
            self.diagnostics_summary.set(summary);
        }
    }

    pub(super) fn clear_lsp_diagnostics(&mut self) {
        let summary_was_non_empty = self.diagnostics_summary.get() != DiagnosticsSummary::default();
        let had_diagnostics = !self.lsp.diagnostics.is_empty();
        let had_diagnostic_result_id = self.lsp.diagnostic_result_id.take().is_some();
        let had_pending_document_diagnostic = self.lsp.pending_document_diagnostic.take().is_some();
        let had_diagnostic_cursor = self.lsp.diagnostic_cursor.take().is_some();
        let had_diagnostics_state = had_diagnostics
            || had_diagnostic_result_id
            || had_pending_document_diagnostic
            || had_diagnostic_cursor
            || summary_was_non_empty;

        self.lsp.diagnostics.clear();
        if had_diagnostics_state {
            self.lsp.diagnostics_revision = self.lsp.diagnostics_revision.saturating_add(1);
        }
        self.set_diagnostics_summary(DiagnosticsSummary::default());
    }

    fn current_lsp_uri(&self) -> Option<String> {
        self.lsp
            .client
            .as_ref()?
            .lock()
            .expect("LSP client mutex poisoned")
            .uri()
    }

    fn lsp_client_and_uri(&self) -> Option<(SharedEditorLspClient, String)> {
        let client = self.lsp.client.clone()?;
        let uri = client.lock().expect("LSP client mutex poisoned").uri()?;
        Some((client, uri))
    }

    fn reset_shared_derived_state(&mut self) {
        self.lsp.pending_semantic_tokens = None;
        self.lsp.pending_folding_ranges = None;
        self.lsp.semantic_result_id = None;
        self.lsp.semantic_data.clear();
        self.lsp.semantic_legend = None;
        self.lsp.last_derived_request_at = None;
    }

    fn maybe_request_shared_derived_state(&mut self) {
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            return;
        };
        if client
            .lock()
            .expect("LSP client mutex poisoned")
            .applies_derived_state_in_poll()
        {
            return;
        }

        let cfg = match self.config.lsp.get() {
            EditorLspMode::External(cfg) | EditorLspMode::Enabled(cfg) => cfg,
            EditorLspMode::Disabled => return,
        };

        let delay = Duration::from_millis(150);
        if let Some(last) = self.lsp.last_derived_request_at
            && last.elapsed() < delay
        {
            return;
        }

        let revision = self.current_inlay_text_revision();
        let mut requested = false;
        let mut lsp = client.lock().expect("LSP client mutex poisoned");
        self.lsp.semantic_legend = lsp.semantic_legend();

        if cfg.semantic_tokens
            && lsp.supports_semantic_tokens()
            && !self
                .lsp
                .pending_semantic_tokens
                .is_some_and(|(_, pending_revision)| pending_revision == revision)
        {
            let request =
                if lsp.supports_semantic_tokens_delta() && self.lsp.semantic_result_id.is_some() {
                    lsp.request_semantic_tokens_delta(&uri, self.lsp.semantic_result_id.clone())
                } else {
                    lsp.request_semantic_tokens_full(&uri)
                };
            if let Ok(id) = request {
                self.lsp.pending_semantic_tokens = Some((id, revision));
                requested = true;
            }
        }

        if cfg.folding_ranges
            && lsp.supports_folding_ranges()
            && !self
                .lsp
                .pending_folding_ranges
                .is_some_and(|(_, pending_revision)| pending_revision == revision)
        {
            if let Ok(id) = lsp.request_folding_ranges(&uri) {
                self.lsp.pending_folding_ranges = Some((id, revision));
                requested = true;
            }
        }

        if requested {
            self.lsp.last_derived_request_at = Some(Instant::now());
        }
    }

    fn handle_shared_derived_response(
        &mut self,
        id: u64,
        method: &str,
        result: Option<&serde_json::Value>,
        error: Option<&editor_core_lsp::LspResponseError>,
    ) -> bool {
        if let Some((pending_id, revision)) = self.lsp.pending_semantic_tokens
            && pending_id == id
            && matches!(
                method,
                "textDocument/semanticTokens/full"
                    | "textDocument/semanticTokens/full/delta"
                    | "textDocument/semanticTokens/range"
            )
        {
            self.lsp.pending_semantic_tokens = None;
            if error.is_some() || revision != self.current_inlay_text_revision() {
                return true;
            }
            let value = result.unwrap_or(&serde_json::Value::Null);
            let line_index = self.state_manager.editor().line_index().clone();
            let baseline = self.lsp.semantic_data.clone();
            if let Some(update) = crate::lsp_client::semantic_tokens_update_for_view(
                value,
                &baseline,
                self.lsp.semantic_legend.as_ref(),
                &line_index,
            ) {
                self.lsp.semantic_result_id = update.result_id;
                self.lsp.semantic_data = update.data;
                if let Some(edit) = update.edit {
                    self.state_manager.apply_processing_edits([edit]);
                }
            } else if value
                .get("edits")
                .and_then(serde_json::Value::as_array)
                .is_some()
            {
                self.lsp.semantic_result_id = None;
                self.lsp.semantic_data.clear();
            }
            return true;
        }

        if let Some((pending_id, revision)) = self.lsp.pending_folding_ranges
            && pending_id == id
            && method == "textDocument/foldingRange"
        {
            self.lsp.pending_folding_ranges = None;
            if error.is_some() || revision != self.current_inlay_text_revision() {
                return true;
            }
            let value = result.unwrap_or(&serde_json::Value::Null);
            let edit = crate::lsp_client::folding_ranges_edit_for_view(value);
            self.state_manager.apply_processing_edits([edit]);
            return true;
        }

        false
    }

    pub(super) fn maybe_start_or_stop_lsp(&mut self) {
        if !self.config.lsp.check_dirty(&mut self.lsp_observer) {
            return;
        }

        match self.config.lsp.get() {
            EditorLspMode::Disabled => {
                self.lsp.client = None;
                self.reset_inlay_hint_tracking();
                self.reset_shared_derived_state();
                editor_core_lsp::clear_lsp_state(&mut self.state_manager);
                self.clear_lsp_diagnostics();
                self.maybe_apply_syntax_highlighting();
                self.hide_popups();
            }
            EditorLspMode::Enabled(cfg) => {
                // Best-effort restart on changes.
                self.lsp.client = None;
                self.reset_inlay_hint_tracking();
                self.reset_shared_derived_state();
                editor_core_lsp::clear_lsp_state(&mut self.state_manager);
                self.clear_lsp_diagnostics();
                self.hide_popups();
                self.start_lsp_session(cfg);
            }
            EditorLspMode::External(_) => {}
        }
    }

    pub(super) fn start_lsp_session(&mut self, cfg: crate::config::EditorLspConfig) {
        if cfg.command.is_empty() {
            return;
        }

        let program = cfg.command[0].clone();
        let args = cfg.command.iter().skip(1).cloned().collect::<Vec<_>>();

        let mut cmd = ProcessCommand::new(&program);
        cmd.args(args);
        cmd.stderr(std::process::Stdio::null());

        let token_types = vec![
            "namespace",
            "type",
            "class",
            "enum",
            "interface",
            "struct",
            "typeParameter",
            "parameter",
            "variable",
            "property",
            "enumMember",
            "event",
            "function",
            "method",
            "macro",
            "keyword",
            "modifier",
            "comment",
            "string",
            "number",
            "regexp",
            "operator",
        ];

        let token_modifiers = vec![
            "declaration",
            "definition",
            "readonly",
            "static",
            "deprecated",
            "abstract",
            "async",
            "modification",
            "documentation",
            "defaultLibrary",
        ];

        let workspace_folders = cfg
            .workspace_folders
            .iter()
            .map(|uri| json!({ "uri": uri, "name": uri }))
            .collect::<Vec<_>>();

        let init_params = json!({
            "processId": std::process::id(),
            "rootUri": cfg.root_uri,
            "workspaceFolders": workspace_folders.clone(),
            "capabilities": {
                "workspace": {
                    "configuration": true,
                    "workspaceFolders": true,
                    "symbol": { "dynamicRegistration": false },
                },
                "textDocument": {
                    "hover": { "dynamicRegistration": false },
                    "completion": {
                        "dynamicRegistration": false,
                        "completionItem": { "snippetSupport": false },
                    },
                    "semanticTokens": {
                        "dynamicRegistration": false,
                        "requests": { "range": false, "full": { "delta": false } },
                        "tokenTypes": token_types,
                        "tokenModifiers": token_modifiers,
                        "formats": ["relative"],
                        "multilineTokenSupport": true,
                        "overlappingTokenSupport": false,
                    },
                    "foldingRange": {
                        "dynamicRegistration": false,
                        "lineFoldingOnly": true,
                    },
                    "definition": { "dynamicRegistration": false },
                    "declaration": { "dynamicRegistration": false },
                    "typeDefinition": { "dynamicRegistration": false },
                    "implementation": { "dynamicRegistration": false },
                    "references": { "dynamicRegistration": false },
                    "documentSymbol": { "dynamicRegistration": false },
                    "codeAction": { "dynamicRegistration": false },
                    "inlayHint": { "dynamicRegistration": false, "resolveSupport": { "properties": ["tooltip", "textEdits"] } },
                },
            },
            "clientInfo": { "name": "atto-ui editor" },
        });

        let start = editor_core_lsp::LspSessionStartOptions {
            cmd,
            workspace_folders,
            initialize_params: init_params,
            initialize_timeout: cfg.initialize_timeout,
            document: editor_core_lsp::LspDocument {
                uri: cfg.document_uri.clone(),
                language_id: cfg.language_id.clone(),
                version: 1,
            },
            initial_text: self.state_manager.editor().get_text(),
        };

        if let Ok(mut session) = editor_core_lsp::LspSession::start(start) {
            session.set_auto_refresh_options(editor_core_lsp::editor::LspAutoRefreshOptions {
                semantic_tokens: cfg.semantic_tokens,
                folding_ranges: cfg.folding_ranges,
                delay: Duration::from_millis(150),
            });
            self.lsp.client = Some(shared_lsp_client(LocalLspClient::new(session)));
        }
    }

    fn handle_lsp_hover_response(&mut self, value: &serde_json::Value, anchor: HoverAnchor) {
        if self.completion_popup.get().is_some() {
            return;
        }
        if self.code_action_popup.get().is_some() {
            return;
        }
        if self.rename_popup.get().is_some() {
            return;
        }
        if self.lsp.hover_suppressed_position == Some(anchor.position) {
            return;
        }

        // LSP hover: { contents, range? }
        let Some(contents) = value.get("contents") else {
            self.hover_popup.set(None);
            return;
        };

        let text = match hover_contents_to_plain_text(contents) {
            Some(lines) if !lines.is_empty() => lines,
            _ => {
                self.hover_popup.set(None);
                return;
            }
        };

        let rect = self.hover_popup_rect_for_screen_point(anchor.screen, text.as_slice());

        self.hover_popup.set(Some(HoverPopupModel {
            rect,
            anchor: anchor.position,
            contents: crate::popup::LspHoverContents::PlainText(text),
        }));
    }

    fn handle_lsp_completion_response(&mut self, value: &serde_json::Value) {
        if self.rename_popup.get().is_some() {
            self.completion_popup.set(None);
            return;
        }
        self.hide_hover_popup_only();
        self.clear_signature_help_popup();

        // Completion: CompletionList { items } | CompletionItem[].
        let items_value = if let Some(items) = value.get("items") {
            items
        } else {
            value
        };

        let Some(arr) = items_value.as_array() else {
            self.completion_popup.set(None);
            return;
        };

        let max_items = self.config.completion.max_items.get().max(1);
        let mut items = Vec::new();
        for item in arr.iter().take(max_items) {
            let label = item
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if label.is_empty() {
                continue;
            }
            let detail = item
                .get("detail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            items.push(crate::popup::CompletionItem {
                label,
                detail,
                edit: LspCompletionItemEdit::Raw(item.clone()),
            });
        }

        if items.is_empty() {
            self.completion_popup.set(None);
            return;
        }

        let Some(rect) = self.completion_popup_rect_for_cursor(items.len()) else {
            self.completion_popup.set(None);
            return;
        };

        self.completion_popup.set(Some(CompletionPopupModel {
            rect,
            items,
            selected: 0,
            scroll: 0,
            accept: None,
        }));
    }

    fn handle_lsp_signature_help_response(&mut self, value: &serde_json::Value) {
        if self.completion_popup.get().is_some()
            || self.code_action_popup.get().is_some()
            || self.rename_popup.get().is_some()
        {
            self.signature_help_popup.set(None);
            return;
        }
        self.hide_hover_popup_only();

        let Some(help) = editor_core_lsp::signature_help_from_value(value) else {
            self.signature_help_popup.set(None);
            return;
        };
        if help.signatures.is_empty() {
            self.signature_help_popup.set(None);
            return;
        }

        let active_signature = active_signature_index(help.active_signature, help.signatures.len());
        let active_parameter = active_parameter_index(
            help.active_parameter,
            help.signatures
                .get(active_signature)
                .map(|signature| signature.parameters.len())
                .unwrap_or_default(),
        );
        let Some(rect) =
            self.signature_help_popup_rect_for_cursor(&help.signatures[active_signature])
        else {
            self.signature_help_popup.set(None);
            return;
        };

        self.signature_help_popup.set(Some(SignatureHelpPopupModel {
            rect,
            signatures: help.signatures,
            active_signature: Some(active_signature),
            active_parameter,
        }));
    }

    fn handle_lsp_code_action_response(&mut self, value: &serde_json::Value) {
        if self.rename_popup.get().is_some() {
            self.lsp.code_action_items.clear();
            self.code_action_popup.set(None);
            return;
        }
        self.hide_hover_popup_only();
        self.completion_popup.set(None);
        self.clear_signature_help_popup();

        let mut items = editor_core_lsp::code_action_items_from_value(value);
        items.sort_by_key(|item| !code_action_item_is_preferred(item));

        let views = items.iter().map(code_action_item_view).collect::<Vec<_>>();
        if views.is_empty() {
            self.lsp.code_action_items.clear();
            self.code_action_popup.set(None);
            return;
        }

        let Some(rect) = self.code_action_popup_rect_for_cursor(&views) else {
            self.lsp.code_action_items.clear();
            self.code_action_popup.set(None);
            return;
        };

        self.lsp.code_action_items = items;
        self.code_action_popup.set(Some(CodeActionPopupModel {
            rect,
            items: views,
            selected: 0,
            scroll: 0,
            accept: None,
        }));
    }

    pub(super) fn hide_hover_popup_only(&mut self) {
        if self.hover_popup.get().is_some() {
            self.hover_popup.set(None);
        }
        self.lsp.hover_due = None;
        self.lsp.hover_pending_request = None;
        self.lsp.hover_target = None;
        self.lsp.hover_requested = None;
    }

    pub(super) fn clear_signature_help_popup(&mut self) {
        if self.signature_help_popup.get().is_some() {
            self.signature_help_popup.set(None);
        }
        self.lsp.pending_signature_help = None;
        self.lsp.signature_help_requested_position = None;
    }

    pub(super) fn consume_hover_popup_dismissed(&mut self) {
        let Some(pos) = self.hover_popup_dismissed.get() else {
            return;
        };
        self.hover_popup_dismissed.set(None);

        // Suppress re-showing at the same hover position until the mouse moves elsewhere.
        self.lsp.hover_suppressed_position = Some(pos);
        self.lsp.hover_due = None;
        self.lsp.hover_pending_request = None;
        self.lsp.hover_target = None;
        self.lsp.hover_requested = None;
    }

    pub(super) fn update_hover_anchor(&mut self, pos: Position, screen: (u16, u16)) {
        if self.lsp.hover_suppressed_position.is_some_and(|p| p != pos) {
            self.lsp.hover_suppressed_position = None;
        }

        let prev_pos = self.lsp.hover_anchor.map(|a| a.position);
        self.lsp.hover_anchor = Some(HoverAnchor {
            position: pos,
            screen,
        });

        // When the hovered position changes, any visible tooltip and any in-flight request become
        // stale. Don't treat this as an explicit dismissal: allow the tooltip to show again after
        // the normal idle delay.
        if prev_pos != Some(pos) {
            if self.hover_popup.get().is_some() {
                self.hover_popup.set(None);
            }
            self.lsp.hover_due = None;
            self.lsp.hover_pending_request = None;
            self.lsp.hover_target = None;
            self.lsp.hover_requested = None;
            return;
        }

        // Same token/position: keep the tooltip close to the mouse.
        if let Some(mut popup) = self.hover_popup.get()
            && popup.anchor == pos
        {
            popup.rect = self.hover_popup_rect_for_screen_point(screen, popup.contents.lines());
            self.hover_popup.set(Some(popup));
        }
    }

    fn request_hover_at_anchor(&mut self, anchor: HoverAnchor) {
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            return;
        };
        let pos = anchor.position;
        if let Ok(id) = client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_hover(
                &uri,
                self.state_manager.editor().line_index(),
                pos.line,
                pos.column,
            )
        {
            self.lsp.hover_pending_request = Some(id);
            self.lsp.hover_requested = Some(anchor);
        }
    }

    pub(super) fn request_hover_now(&mut self) {
        let Some(screen) = self.cursor_screen_position().and_then(|p| p) else {
            return;
        };
        self.request_hover_at_anchor(HoverAnchor {
            position: self.active_cursor_position(),
            screen,
        });
    }

    pub(super) fn request_completion_now(&mut self) {
        if !self.config.completion.enabled.get() {
            return;
        }
        self.clear_signature_help_popup();
        self.code_action_popup.set(None);
        self.lsp.pending_code_action = None;
        self.lsp.code_action_items.clear();
        self.rename_popup.set(None);
        self.lsp.rename_target = None;
        self.lsp.pending_prepare_rename = None;
        self.lsp.pending_rename = None;
        let pos = self.active_cursor_position();
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            return;
        };
        if let Ok(id) = client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_completion(
                &uri,
                self.state_manager.editor().line_index(),
                pos.line,
                pos.column,
            )
        {
            self.lsp.completion_pending_request = Some(id);
            self.lsp.completion_requested_position = Some(pos);
        }
    }

    pub(super) fn request_signature_help_now(&mut self) {
        self.hide_hover_popup_only();
        if self.completion_popup.get().is_some()
            || self.code_action_popup.get().is_some()
            || self.rename_popup.get().is_some()
        {
            self.clear_signature_help_popup();
            return;
        }

        let pos = self.active_cursor_position();
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            self.clear_signature_help_popup();
            return;
        };
        match client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_signature_help(
                &uri,
                self.state_manager.editor().line_index(),
                pos.line,
                pos.column,
            ) {
            Ok(id) => {
                self.lsp.pending_signature_help = Some(id);
                self.lsp.signature_help_requested_position = Some(pos);
            }
            Err(_) => self.clear_signature_help_popup(),
        }
    }

    pub(super) fn request_code_action_now(&mut self) {
        let (start, end) = self.code_action_request_offsets();
        self.hide_hover_popup_only();
        self.completion_popup.set(None);
        self.lsp.completion_pending_request = None;
        self.lsp.completion_requested_position = None;
        self.clear_signature_help_popup();
        self.code_action_popup.set(None);
        self.lsp.code_action_items.clear();
        self.rename_popup.set(None);
        self.lsp.rename_target = None;
        self.lsp.pending_prepare_rename = None;
        self.lsp.pending_rename = None;

        let Some((client, uri)) = self.lsp_client_and_uri() else {
            return;
        };

        let context = json!({ "diagnostics": [] });
        if let Ok(id) = client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_code_action(
                &uri,
                self.state_manager.editor().line_index(),
                start,
                end,
                context,
            )
        {
            self.lsp.pending_code_action = Some(id);
        }
    }

    pub fn request_format_document_now(&mut self, report_unavailable: bool) -> bool {
        self.hide_popups();
        let options = self.formatting_options();
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            if report_unavailable {
                self.events.push(EditorEvent::LspMessage {
                    message: "Format-on-save requires an active LSP session".to_string(),
                });
                self.events.push(EditorEvent::FormatFinished {
                    success: false,
                    changed: false,
                });
            }
            return false;
        };

        match client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_formatting(&uri, options)
        {
            Ok(id) => {
                let timeout = self.config.formatting_timeout.get();
                self.lsp.pending_formatting = Some(PendingFormatting {
                    id,
                    deadline: Instant::now() + timeout,
                });
                true
            }
            Err(err) => {
                self.events.push(EditorEvent::LspMessage {
                    message: format!("Formatting request failed: {err}"),
                });
                self.events.push(EditorEvent::FormatFinished {
                    success: false,
                    changed: false,
                });
                false
            }
        }
    }

    pub(super) fn request_prepare_rename_now(&mut self) {
        self.hide_hover_popup_only();
        self.completion_popup.set(None);
        self.lsp.completion_pending_request = None;
        self.lsp.completion_requested_position = None;
        self.clear_signature_help_popup();
        self.code_action_popup.set(None);
        self.lsp.pending_code_action = None;
        self.lsp.code_action_items.clear();
        self.rename_popup.set(None);
        self.lsp.pending_prepare_rename = None;
        self.lsp.pending_rename = None;
        self.lsp.rename_target = None;

        let position = self.active_cursor_position();
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            self.events.push(EditorEvent::LspMessage {
                message: "Rename requires an active LSP session".to_string(),
            });
            return;
        };

        let line_index = self.state_manager.editor().line_index();
        match client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_prepare_rename(&uri, line_index, position.line, position.column)
        {
            Ok(id) => {
                self.lsp.pending_prepare_rename = Some((id, RenameTarget { position }));
            }
            Err(err) => self.events.push(EditorEvent::LspMessage {
                message: format!("Rename prepare request failed: {err}"),
            }),
        }
    }

    pub(super) fn cancel_rename_popup(&mut self) {
        self.rename_popup.set(None);
        self.lsp.rename_target = None;
    }

    pub(super) fn submit_rename_popup(&mut self) {
        let Some(model) = self.rename_popup.get() else {
            return;
        };
        let new_name = model.value;
        if new_name.is_empty() {
            self.events.push(EditorEvent::LspMessage {
                message: "Rename target cannot be empty".to_string(),
            });
            return;
        }
        let Some(target) = self.lsp.rename_target else {
            self.rename_popup.set(None);
            self.events.push(EditorEvent::LspMessage {
                message: "Rename target is no longer available".to_string(),
            });
            return;
        };
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            self.rename_popup.set(None);
            self.lsp.rename_target = None;
            self.events.push(EditorEvent::LspMessage {
                message: "Rename requires an active LSP session".to_string(),
            });
            return;
        };

        let line_index = self.state_manager.editor().line_index();
        match client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_rename(
                &uri,
                line_index,
                target.position.line,
                target.position.column,
                new_name,
            ) {
            Ok(id) => {
                self.rename_popup.set(None);
                self.lsp.pending_rename = Some(id);
            }
            Err(err) => {
                self.rename_popup.set(None);
                self.lsp.rename_target = None;
                self.events.push(EditorEvent::LspMessage {
                    message: format!("Rename request failed: {err}"),
                });
            }
        }
    }

    pub(super) fn insert_rename_popup_char(&mut self, ch: char) {
        let Some(mut model) = self.rename_popup.get() else {
            return;
        };
        if model.replace_on_input {
            model.value.clear();
            model.cursor = 0;
            model.replace_on_input = false;
        }
        insert_char_at(&mut model.value, model.cursor, ch);
        model.cursor = model.cursor.saturating_add(1);
        self.rename_popup.set(Some(model));
    }

    pub(super) fn backspace_rename_popup(&mut self) {
        let Some(mut model) = self.rename_popup.get() else {
            return;
        };
        if model.replace_on_input {
            model.value.clear();
            model.cursor = 0;
            model.replace_on_input = false;
        } else if model.cursor > 0 {
            model.cursor = model.cursor.saturating_sub(1);
            remove_char_at(&mut model.value, model.cursor);
        }
        self.rename_popup.set(Some(model));
    }

    pub(super) fn delete_rename_popup(&mut self) {
        let Some(mut model) = self.rename_popup.get() else {
            return;
        };
        if model.replace_on_input {
            model.value.clear();
            model.cursor = 0;
            model.replace_on_input = false;
        } else {
            remove_char_at(&mut model.value, model.cursor);
        }
        self.rename_popup.set(Some(model));
    }

    pub(super) fn move_rename_popup_cursor(&mut self, delta: isize) {
        let Some(mut model) = self.rename_popup.get() else {
            return;
        };
        model.replace_on_input = false;
        let len = model.value.chars().count() as isize;
        let next = (model.cursor as isize + delta).clamp(0, len);
        model.cursor = next as usize;
        self.rename_popup.set(Some(model));
    }

    pub(super) fn move_rename_popup_cursor_to(&mut self, cursor: usize) {
        let Some(mut model) = self.rename_popup.get() else {
            return;
        };
        model.replace_on_input = false;
        model.cursor = cursor.min(model.value.chars().count());
        self.rename_popup.set(Some(model));
    }

    pub fn request_document_symbols(&mut self) -> bool {
        self.hide_popups();
        self.lsp.pending_document_symbols = None;
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            return false;
        };
        match client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_document_symbols(&uri)
        {
            Ok(id) => {
                self.lsp.pending_document_symbols = Some(id);
                true
            }
            Err(_) => false,
        }
    }

    pub fn request_workspace_symbols(&mut self, query: impl Into<String>) -> bool {
        let query = query.into();
        if query.trim().is_empty() {
            return false;
        }
        self.hide_popups();
        self.lsp.pending_workspace_symbols = None;
        let Some(client) = self.lsp.client.clone() else {
            return false;
        };
        match client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_workspace_symbol(query.clone())
        {
            Ok(id) => {
                self.lsp.pending_workspace_symbols = Some((id, query));
                true
            }
            Err(_) => false,
        }
    }

    pub(super) fn maybe_request_inlay_hints(&mut self, focused: bool) {
        if !focused || !self.config.inlay_hints.enabled.get() {
            return;
        }
        if self.lsp.pending_inlay_hints.is_some() {
            return;
        }

        let Some(key) = self.current_inlay_request_key() else {
            return;
        };
        if self.lsp.last_inlay_range == Some(key.range)
            && self.lsp.last_inlay_text_revision == Some(key.text_revision)
        {
            return;
        }

        let delay = self.config.inlay_hints.refresh_delay.get();
        if let Some(last) = self.lsp.last_inlay_request_at
            && last.elapsed() < delay
        {
            return;
        }

        let Some((client, uri)) = self.lsp_client_and_uri() else {
            return;
        };
        let line_index = self.state_manager.editor().line_index();
        let requested_at = Instant::now();
        match client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_inlay_hints(&uri, line_index, key.range.start, key.range.end)
        {
            Ok(id) => {
                self.lsp.pending_inlay_hints = Some(id);
                self.lsp.pending_inlay_key = Some(key);
                self.lsp.last_inlay_range = Some(key.range);
                self.lsp.last_inlay_text_revision = Some(key.text_revision);
                self.lsp.last_inlay_request_at = Some(requested_at);
            }
            Err(err) => {
                self.lsp.last_inlay_range = Some(key.range);
                self.lsp.last_inlay_text_revision = Some(key.text_revision);
                self.lsp.last_inlay_request_at = Some(requested_at);
                self.events.push(EditorEvent::LspMessage {
                    message: format!("Inlay hints request failed: {err}"),
                });
            }
        }
    }

    pub(super) fn toggle_lsp_inlay_hints(&mut self) {
        let enabled = self.config.inlay_hints.enabled.get();
        self.config.inlay_hints.enabled.set(!enabled);
        self.reset_inlay_hint_tracking();
        if enabled {
            self.clear_lsp_inlay_hints();
        }
    }

    pub(super) fn reset_inlay_hint_tracking(&mut self) {
        self.lsp.pending_inlay_hints = None;
        self.lsp.pending_inlay_key = None;
        self.lsp.last_inlay_range = None;
        self.lsp.last_inlay_text_revision = None;
        self.lsp.last_inlay_request_at = None;
    }

    pub(super) fn clear_lsp_inlay_hints(&mut self) {
        let line_index = self.state_manager.editor().line_index().clone();
        let edit = editor_core_lsp::lsp_inlay_hints_to_processing_edit(
            &line_index,
            &serde_json::Value::Null,
        );
        self.state_manager.apply_processing_edits([edit]);
    }

    fn apply_lsp_inlay_hints_response(&mut self, value: &serde_json::Value) {
        let line_index = self.state_manager.editor().line_index().clone();
        let edit = editor_core_lsp::lsp_inlay_hints_to_processing_edit(&line_index, value);
        self.state_manager.apply_processing_edits([edit]);
    }

    fn current_inlay_request_key(&self) -> Option<InlayHintRequestKey> {
        Some(InlayHintRequestKey {
            range: self.current_inlay_range()?,
            text_revision: self.current_inlay_text_revision(),
        })
    }

    fn current_inlay_range(&self) -> Option<InlayHintRange> {
        let editor = self.state_manager.editor();
        let total_visual = editor.visual_line_count();
        let viewport_height = self.viewport_size.1 as usize;
        if total_visual == 0 || viewport_height == 0 {
            return None;
        }

        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        let first_visual = scroll_top.min(total_visual.saturating_sub(1));
        let last_visual = scroll_top
            .saturating_add(viewport_height.saturating_sub(1))
            .min(total_visual.saturating_sub(1));

        let (first_line, _first_visual_in_line) = editor.visual_to_logical_line(first_visual);
        let (last_line, _last_visual_in_line) = editor.visual_to_logical_line(last_visual);
        let line_index = editor.line_index();
        let start = line_index.position_to_char_offset(first_line, 0);
        let last_line_text = line_index.get_line_text(last_line).unwrap_or_default();
        let end = line_index.position_to_char_offset(last_line, last_line_text.chars().count());

        Some(InlayHintRange {
            start: start.min(end),
            end: end.max(start),
        })
    }

    fn current_inlay_text_revision(&self) -> InlayTextRevision {
        let text = self.state_manager.editor().get_text();
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        InlayTextRevision {
            len: text.len(),
            hash: hasher.finish(),
        }
    }

    fn formatting_options(&self) -> serde_json::Value {
        let tab_width = self.config.indent.tab_width.get().max(1);
        let insert_spaces = self.config.indent.insert_spaces.get();
        editor_core_lsp::lsp_formatting_options(tab_width, insert_spaces)
    }

    fn full_document_lsp_change(&self, new_text: String) -> Option<LspContentChange> {
        self.current_lsp_uri()?;
        let line_index = self.state_manager.editor().line_index();
        let old_char_count = self.state_manager.editor().char_count();
        Some(LspContentChange {
            range: editor_core_lsp::LspRange::new(
                lsp_position_for_char_offset(line_index, 0),
                lsp_position_for_char_offset(line_index, old_char_count),
            ),
            text: new_text,
        })
    }

    fn finish_pending_formatting_failure(&mut self, message: String) {
        if self.lsp.pending_formatting.take().is_none() {
            return;
        }

        self.events.push(EditorEvent::LspMessage { message });
        self.events.push(EditorEvent::FormatFinished {
            success: false,
            changed: false,
        });
    }

    fn maybe_timeout_pending_formatting(&mut self) {
        let timed_out = self
            .lsp
            .pending_formatting
            .is_some_and(|pending| Instant::now() >= pending.deadline);
        if timed_out {
            self.finish_pending_formatting_failure("Formatting timed out".to_string());
        }
    }

    fn handle_lsp_formatting_response(&mut self, value: &serde_json::Value) {
        let lsp_edits = editor_core_lsp::text_edits_from_value(value);
        if lsp_edits.is_empty() {
            self.events.push(EditorEvent::FormatFinished {
                success: true,
                changed: false,
            });
            return;
        }

        let line_index = self.state_manager.editor().line_index().clone();
        let edits = lsp_edits
            .iter()
            .map(|edit| {
                let (start, end) =
                    editor_core_lsp::char_offsets_for_lsp_range(&line_index, &edit.range);
                TextEditSpec {
                    start,
                    end,
                    text: edit.new_text.clone(),
                }
            })
            .collect::<Vec<_>>();

        // `ApplyTextEdits` applies each edit via a separate internal `execute`, so only the last
        // sub-edit's `TextDelta` survives in `last_text_delta` — the delta helper can't represent
        // the whole batch. A full-document replacement is exact here and keeps the LSP mirror in
        // sync (`did_change_many` advances it by the same replacement).
        let full_lsp_change = self.full_document_lsp_change(String::new());

        let before_text = self.state_manager.editor().get_text();
        let apply_result = self
            .state_manager
            .execute(Command::Edit(EditCommand::ApplyTextEdits { edits }));
        if let Err(err) = apply_result {
            self.events.push(EditorEvent::LspMessage {
                message: format!("Formatting failed to apply edits: {err}"),
            });
            self.events.push(EditorEvent::FormatFinished {
                success: false,
                changed: false,
            });
            return;
        }

        let after_text = self.state_manager.editor().get_text();
        let changed = after_text != before_text;
        if changed {
            self.config.text.set(after_text.clone());
            self.last_insert_time = None;
            self.maybe_apply_syntax_highlighting();
            self.adjust_scroll();
            self.hide_hover_popup_only();
            if let Some(mut change) = full_lsp_change {
                change.text = after_text;
                self.lsp_did_change(change);
            }
        }

        self.events.push(EditorEvent::FormatFinished {
            success: true,
            changed,
        });
    }

    fn code_action_request_offsets(&self) -> (usize, usize) {
        let cursor_state = self.state_manager.get_cursor_state();
        let primary = cursor_state.primary_selection_index;
        let selection = cursor_state.selections.get(primary);
        selection
            .filter(|s| s.start != s.end)
            .map(|s| self.selection_offsets(s))
            .unwrap_or_else(|| {
                let offset = self.cursor_offset();
                (offset, offset)
            })
    }

    pub(super) fn request_goto(&mut self, kind: EditorLspGotoKind) {
        let pos = self.active_cursor_position();
        let Some((client, uri)) = self.lsp_client_and_uri() else {
            return;
        };
        let line_index = &self.state_manager.editor().line_index();
        let request = match kind {
            EditorLspGotoKind::Definition => client
                .lock()
                .expect("LSP client mutex poisoned")
                .request_definition(&uri, line_index, pos.line, pos.column),
            EditorLspGotoKind::Declaration => client
                .lock()
                .expect("LSP client mutex poisoned")
                .request_declaration(&uri, line_index, pos.line, pos.column),
            EditorLspGotoKind::TypeDefinition => client
                .lock()
                .expect("LSP client mutex poisoned")
                .request_type_definition(&uri, line_index, pos.line, pos.column),
            EditorLspGotoKind::Implementation => client
                .lock()
                .expect("LSP client mutex poisoned")
                .request_implementation(&uri, line_index, pos.line, pos.column),
            EditorLspGotoKind::References => client
                .lock()
                .expect("LSP client mutex poisoned")
                .request_references(&uri, line_index, pos.line, pos.column, true),
        };
        if let Ok(id) = request {
            self.lsp.pending_goto = Some((id, kind));
        }
    }

    pub(super) fn schedule_hover_after_delay(&mut self) {
        if self.hover_popup.get().is_some() {
            return;
        }
        if self.lsp.completion_pending_request.is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.lsp.pending_signature_help.is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.completion_popup.get().is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.signature_help_popup.get().is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.code_action_popup.get().is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.rename_popup.get().is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if !self.config.hover.enabled.get() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.lsp.client.is_none() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.lsp.hover_pending_request.is_some() {
            return;
        }

        let Some(anchor) = self.lsp.hover_anchor else {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        };
        if self.lsp.hover_suppressed_position == Some(anchor.position) {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }

        let delay = self.config.hover.delay.get();
        self.lsp.hover_due = Some(Instant::now() + delay);
        self.lsp.hover_target = Some(anchor);
    }

    pub(super) fn maybe_fire_hover(&mut self) {
        let Some(due) = self.lsp.hover_due else {
            return;
        };
        if Instant::now() < due {
            return;
        }
        if self.lsp.hover_pending_request.is_some() {
            return;
        }
        if self.lsp.completion_pending_request.is_some() {
            return;
        }
        if self.lsp.pending_signature_help.is_some() {
            return;
        }
        if self.hover_popup.get().is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.completion_popup.get().is_some() {
            return;
        }
        if self.signature_help_popup.get().is_some() {
            return;
        }
        if self.code_action_popup.get().is_some() {
            return;
        }
        if self.rename_popup.get().is_some() {
            return;
        }

        let Some(target) = self.lsp.hover_target else {
            self.lsp.hover_due = None;
            return;
        };

        if self.lsp.hover_anchor.map(|a| a.position) != Some(target.position) {
            self.schedule_hover_after_delay();
            return;
        }
        if self.lsp.hover_suppressed_position == Some(target.position) {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }

        self.lsp.hover_due = None;
        self.lsp.hover_target = None;
        self.request_hover_at_anchor(target);
    }

    pub(super) fn process_completion_accept(&mut self) {
        let Some(mut popup) = self.completion_popup.get() else {
            return;
        };
        let Some(idx) = popup.accept.take() else {
            return;
        };
        self.completion_popup.set(Some(popup.clone()));
        self.apply_completion_index(idx);
        self.completion_popup.set(None);
    }

    pub(super) fn process_code_action_accept(&mut self) {
        let Some(mut popup) = self.code_action_popup.get() else {
            return;
        };
        let Some(idx) = popup.accept.take() else {
            return;
        };
        self.code_action_popup.set(Some(popup.clone()));
        self.apply_code_action_index(idx);
        self.code_action_popup.set(None);
    }

    fn apply_code_action_index(&mut self, idx: usize) {
        if self.config.read_only.get() {
            self.lsp.code_action_items.clear();
            return;
        }
        let Some(item) = self.lsp.code_action_items.get(idx).cloned() else {
            return;
        };
        self.lsp.code_action_items.clear();

        let plan = editor_core_lsp::apply_plan_for_code_action_item(&item);
        let mut edit_applied_or_absent = true;
        if let Some(edit) = plan.edit.as_ref() {
            edit_applied_or_absent = self.apply_code_action_workspace_edit(edit);
        }

        if edit_applied_or_absent && let Some(command) = plan.command {
            self.request_code_action_command(command);
        }
    }

    fn apply_code_action_workspace_edit(&mut self, edit: &serde_json::Value) -> bool {
        let Some(current_uri) = self.current_lsp_uri() else {
            return false;
        };

        let summary = editor_core_lsp::summarize_workspace_edit(edit);
        let unsupported = summary
            .documents
            .iter()
            .filter(|doc| doc.uri != current_uri)
            .map(|doc| doc.uri.clone())
            .collect::<Vec<_>>();
        if !unsupported.is_empty() || summary.documents.len() > 1 {
            self.events.push(EditorEvent::CodeActionMessage {
                message: format!(
                    "Skipped code action: workspace edit targets unsupported URI(s): {}",
                    unsupported.join(", ")
                ),
            });
            return false;
        }

        let before_text = self.state_manager.editor().get_text();
        // `apply_workspace_edit` applies its text edits through multiple internal executes, so
        // `last_text_delta` only holds the final sub-edit. Send a full-document replacement, which
        // is exact and keeps the LSP mirror aligned.
        let full_lsp_change = self.full_document_lsp_change(String::new());
        let lsp_edits = editor_core_lsp::workspace_edit_text_edits_for_uri(edit, &current_uri);
        if editor_core_lsp::apply_text_edits(&mut self.state_manager, &lsp_edits).is_err() {
            self.events.push(EditorEvent::CodeActionMessage {
                message: "Skipped code action: failed to apply workspace edit".to_string(),
            });
            return false;
        }

        let after_text = self.state_manager.editor().get_text();
        if after_text != before_text {
            self.config.text.set(after_text.clone());
            self.last_insert_time = None;
            self.maybe_apply_syntax_highlighting();
            self.adjust_scroll();
            self.hide_hover_popup_only();
            if let Some(mut change) = full_lsp_change {
                change.text = after_text;
                self.lsp_did_change(change);
            }
        }

        true
    }

    fn request_code_action_command(&mut self, command: editor_core_lsp::LspCommand) {
        let Some(client) = self.lsp.client.clone() else {
            return;
        };
        let _ = client
            .lock()
            .expect("LSP client mutex poisoned")
            .request_execute_command(command.command, command.arguments);
    }

    fn apply_completion_index(&mut self, idx: usize) {
        let Some(popup) = self.completion_popup.get() else {
            return;
        };
        let Some(item) = popup.items.get(idx) else {
            return;
        };

        let LspCompletionItemEdit::Raw(raw) = &item.edit;
        let Some(obj) = raw.as_object() else {
            return;
        };

        // Basic insertion strategy:
        // - prefer `textEdit` if present (TextEdit shape)
        // - else use `insertText`
        // - else insert `label`
        if let Some(text_edit) = obj.get("textEdit") {
            // `apply_text_edits` runs one execute per edit, so `last_text_delta` can't capture the
            // batch; a full-document replacement is exact and keeps the LSP mirror in sync.
            let full_lsp_change = self.full_document_lsp_change(String::new());

            let edits = editor_core_lsp::text_edits_from_value(&serde_json::Value::Array(vec![
                text_edit.clone(),
            ]));
            let _ = editor_core_lsp::apply_text_edits(&mut self.state_manager, &edits);
            let after_text = self.state_manager.editor().get_text();
            self.config.text.set(after_text.clone());
            self.maybe_apply_syntax_highlighting();
            self.hide_hover_popup_only();
            if let Some(mut change) = full_lsp_change {
                change.text = after_text;
                self.lsp_did_change(change);
            }
            return;
        }

        if let Some(insert_text) = obj.get("insertText").and_then(|v| v.as_str()) {
            self.insert_text(insert_text);
            return;
        }

        self.insert_text(item.label.as_str());
    }

    pub(super) fn hover_popup_rect_for_screen_point(
        &self,
        screen: (u16, u16),
        lines: &[String],
    ) -> Rect {
        let (x, y) = screen;
        let max_line = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let width = (max_line + 2).clamp(10, 80) as u16;
        let height = (lines.len() + 2).clamp(3, 12) as u16;

        Rect {
            x: x.saturating_add(1),
            y: y.saturating_add(1),
            width,
            height,
        }
    }

    fn completion_popup_rect_for_cursor(&self, item_count: usize) -> Option<Rect> {
        let (cursor_x, cursor_y) = self.cursor_screen_position()??;
        let height = (item_count.min(8) + 2).max(3) as u16;
        let width = 40u16;
        Some(Rect {
            x: cursor_x.saturating_add(1),
            y: cursor_y.saturating_add(1),
            width,
            height,
        })
    }

    fn signature_help_popup_rect_for_cursor(
        &self,
        signature: &editor_core_lsp::LspSignatureInformation,
    ) -> Option<Rect> {
        let (cursor_x, cursor_y) = self.cursor_screen_position()??;
        let width = (signature.label.chars().count() + 2).clamp(16, 80) as u16;
        let desired = Rect {
            x: cursor_x.saturating_add(1),
            y: cursor_y.saturating_add(1),
            width,
            height: 3,
        };
        let bounds = self.inline_popup_bounds()?;
        Some(clamp_rect_to_bounds(desired, bounds))
    }

    fn inline_popup_bounds(&self) -> Option<Rect> {
        let area = self.last_area?;
        let bar_height = self.search_bar_height().min(area.height);
        Some(Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.saturating_sub(bar_height),
        })
    }

    fn code_action_popup_rect_for_cursor(&self, items: &[CodeActionItemView]) -> Option<Rect> {
        let (cursor_x, cursor_y) = self.cursor_screen_position()??;
        let height = (items.len().min(8) + 2).max(3) as u16;
        let max_line = items
            .iter()
            .map(|item| {
                let kind_len = item
                    .kind
                    .as_ref()
                    .map(|k| k.chars().count() + 2)
                    .unwrap_or(0);
                2 + item.title.chars().count() + kind_len
            })
            .max()
            .unwrap_or(20);
        let width = (max_line + 2).clamp(24, 80) as u16;
        Some(Rect {
            x: cursor_x.saturating_add(1),
            y: cursor_y.saturating_add(1),
            width,
            height,
        })
    }

    fn rename_popup_rect_for_cursor(&self, value: &str) -> Option<Rect> {
        let (cursor_x, cursor_y) = self.cursor_screen_position()??;
        let width = (value.chars().count() + "Rename: ".len() + 4).clamp(24, 80) as u16;
        Some(Rect {
            x: cursor_x.saturating_add(1),
            y: cursor_y.saturating_add(1),
            width,
            height: 3,
        })
    }

    fn open_rename_popup(&mut self, target: RenameTarget, value: String) {
        let Some(rect) = self.rename_popup_rect_for_cursor(&value) else {
            self.events.push(EditorEvent::LspMessage {
                message: "Rename popup cannot be shown at the current cursor".to_string(),
            });
            return;
        };
        self.hide_hover_popup_only();
        self.completion_popup.set(None);
        self.lsp.completion_pending_request = None;
        self.lsp.completion_requested_position = None;
        self.code_action_popup.set(None);
        self.lsp.pending_code_action = None;
        self.lsp.code_action_items.clear();
        self.lsp.rename_target = Some(target);
        self.rename_popup.set(Some(RenamePopupModel {
            rect,
            cursor: value.chars().count(),
            value,
            replace_on_input: true,
        }));
    }

    fn rename_default_from_prepare_response(&self, value: &serde_json::Value) -> Option<String> {
        if let Some(placeholder) = value.get("placeholder").and_then(|v| v.as_str())
            && !placeholder.is_empty()
        {
            return Some(placeholder.to_string());
        }

        if let Some(range) = value.get("range").and_then(lsp_range_from_value) {
            return self
                .text_for_lsp_range(range)
                .filter(|text| !text.is_empty());
        }

        if value.get("defaultBehavior").and_then(|v| v.as_bool()) == Some(true) {
            return self.current_word_at_cursor();
        }

        lsp_range_from_value(value).and_then(|range| self.text_for_lsp_range(range))
    }

    fn text_for_lsp_range(&self, range: editor_core_lsp::LspRange) -> Option<String> {
        let line_index = self.state_manager.editor().line_index();
        let (start, end) = editor_core_lsp::char_offsets_for_lsp_range(line_index, &range);
        if start == end {
            return None;
        }
        Some(
            self.state_manager
                .editor()
                .get_text()
                .chars()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect(),
        )
    }

    fn current_word_at_cursor(&self) -> Option<String> {
        let pos = self.active_cursor_position();
        let line = self
            .state_manager
            .editor()
            .line_index()
            .get_line_text(pos.line)?;
        word_at_char_column(&line, pos.column)
    }

    pub(super) fn cursor_screen_position(&self) -> Option<Option<(u16, u16)>> {
        let area = self.last_area?;
        let (_gutter, text_area) = self.layout_rects(area);
        if text_area.width == 0 || text_area.height == 0 {
            return Some(None);
        }

        if self.config.inlay_hints.enabled.get()
            && let Some((cursor_x, cursor_y)) = self.cursor_screen_position_from_composed(text_area)
        {
            return Some(Some((cursor_x, cursor_y)));
        }

        let editor = self.state_manager.editor();
        let pos = self.active_cursor_position();
        let Some((cursor_visual_row, cursor_x_in_row)) =
            editor.logical_position_to_visual_allow_virtual(pos.line, pos.column)
        else {
            return Some(None);
        };
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        if cursor_visual_row < scroll_top {
            return Some(None);
        }
        let y = cursor_visual_row.saturating_sub(scroll_top);
        if y >= text_area.height as usize {
            return Some(None);
        }
        let x = cursor_x_in_row.min(text_area.width.saturating_sub(1) as usize) as u16;
        Some(Some((
            text_area.x.saturating_add(x),
            text_area.y.saturating_add(y as u16),
        )))
    }

    fn cursor_screen_position_from_composed(&self, text_area: Rect) -> Option<(u16, u16)> {
        let editor = self.state_manager.editor();
        let pos = self.active_cursor_position();
        let cursor_offset = editor
            .line_index()
            .position_to_char_offset(pos.line, pos.column);
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        let grid = self
            .state_manager
            .get_viewport_content_composed(scroll_top, text_area.height as usize);

        for (row, line) in grid.lines.iter().enumerate() {
            let ComposedLineKind::Document { logical_line, .. } = line.kind else {
                continue;
            };
            if logical_line != pos.line {
                continue;
            }

            let mut x = 0usize;
            for cell in &line.cells {
                match cell.source {
                    ComposedCellSource::Document { offset } if offset >= cursor_offset => {
                        return Some(clamped_text_point(text_area, x, row));
                    }
                    ComposedCellSource::Virtual { anchor_offset }
                        if !cell.styles.is_empty() && anchor_offset >= cursor_offset =>
                    {
                        return Some(clamped_text_point(text_area, x, row));
                    }
                    _ => {
                        x = x.saturating_add(cell.width);
                    }
                }
            }

            if cursor_offset >= line.char_offset_start && cursor_offset <= line.char_offset_end {
                return Some(clamped_text_point(text_area, x, row));
            }
        }

        None
    }

    pub(super) fn layout_rects(&self, area: Rect) -> (Rect, Rect) {
        let show_line_numbers = self.config.show_line_numbers.get();
        let show_folding_markers = self.config.show_folding_markers.get();
        let show_diagnostics = !self.state_manager.editor().diagnostics().is_empty();

        let line_count = self.state_manager.editor().line_index().line_count().max(1);
        let digits = line_count.to_string().len().max(2) as u16;

        let mut gutter_w = 0u16;
        if show_line_numbers {
            gutter_w = gutter_w.saturating_add(digits.saturating_add(1));
        }
        if show_folding_markers {
            gutter_w = gutter_w.saturating_add(2);
        }
        if show_diagnostics {
            gutter_w = gutter_w.saturating_add(2);
        }

        // Add a separator if there is any gutter at all.
        let sep_w = if gutter_w > 0 { 1 } else { 0 };
        let gutter_total = gutter_w.saturating_add(sep_w).min(area.width);

        let gutter = Rect {
            x: area.x,
            y: area.y,
            width: gutter_total,
            height: area.height,
        };
        let text = Rect {
            x: area.x.saturating_add(gutter_total),
            y: area.y,
            width: area.width.saturating_sub(gutter_total),
            height: area.height,
        };
        (gutter, text)
    }
}

// Helper to keep LSP hover parsing out of the main view module.
fn hover_contents_to_plain_text(contents: &serde_json::Value) -> Option<Vec<String>> {
    // Spec: `contents` can be MarkedString | MarkedString[] | MarkupContent.
    if let Some(s) = contents.as_str() {
        return Some(s.lines().map(|l| l.to_string()).collect());
    }

    if let Some(obj) = contents.as_object() {
        // MarkupContent: { kind: "markdown" | "plaintext", value: "..."}
        if let Some(value) = obj.get("value").and_then(|v| v.as_str()) {
            return Some(value.lines().map(|l| l.to_string()).collect());
        }
        // MarkedString: { language, value }
        if let Some(value) = obj.get("value").and_then(|v| v.as_str()) {
            return Some(value.lines().map(|l| l.to_string()).collect());
        }
    }

    if let Some(arr) = contents.as_array() {
        let mut out = Vec::<String>::new();
        for item in arr {
            if let Some(lines) = hover_contents_to_plain_text(item) {
                out.extend(lines);
            }
        }
        return Some(out);
    }

    None
}

fn active_signature_index(active_signature: Option<u32>, len: usize) -> usize {
    let idx = active_signature
        .and_then(|idx| usize::try_from(idx).ok())
        .unwrap_or(0);
    idx.min(len.saturating_sub(1))
}

fn active_parameter_index(active_parameter: Option<u32>, len: usize) -> Option<usize> {
    let idx = active_parameter.and_then(|idx| usize::try_from(idx).ok())?;
    (idx < len).then_some(idx)
}

fn clamp_rect_to_bounds(mut rect: Rect, bounds: Rect) -> Rect {
    if bounds.width == 0 || bounds.height == 0 {
        return Rect {
            x: bounds.x,
            y: bounds.y,
            width: 0,
            height: 0,
        };
    }

    rect.width = rect.width.min(bounds.width);
    rect.height = rect.height.min(bounds.height);

    let max_x = bounds
        .x
        .saturating_add(bounds.width.saturating_sub(rect.width));
    let max_y = bounds
        .y
        .saturating_add(bounds.height.saturating_sub(rect.height));
    rect.x = rect.x.clamp(bounds.x, max_x);
    rect.y = rect.y.clamp(bounds.y, max_y);
    rect
}

fn clamped_text_point(text_area: Rect, x: usize, row: usize) -> (u16, u16) {
    let x = x.min(text_area.width.saturating_sub(1) as usize) as u16;
    let y = row.min(text_area.height.saturating_sub(1) as usize) as u16;
    (text_area.x.saturating_add(x), text_area.y.saturating_add(y))
}

fn code_action_item_is_preferred(item: &LspCodeActionItem) -> bool {
    match item {
        LspCodeActionItem::CodeAction(action) => action.is_preferred,
        LspCodeActionItem::Command(_) => false,
    }
}

fn code_action_item_view(item: &LspCodeActionItem) -> CodeActionItemView {
    match item {
        LspCodeActionItem::CodeAction(action) => CodeActionItemView {
            title: action.title.clone(),
            kind: action.kind.clone(),
            is_preferred: action.is_preferred,
        },
        LspCodeActionItem::Command(command) => CodeActionItemView {
            title: command.title.clone(),
            kind: Some("command".to_string()),
            is_preferred: false,
        },
    }
}

fn lsp_range_from_value(value: &serde_json::Value) -> Option<editor_core_lsp::LspRange> {
    let start = value.get("start")?;
    let end = value.get("end")?;
    Some(editor_core_lsp::LspRange::new(
        lsp_position_from_value(start)?,
        lsp_position_from_value(end)?,
    ))
}

fn lsp_position_from_value(value: &serde_json::Value) -> Option<editor_core_lsp::LspPosition> {
    Some(editor_core_lsp::LspPosition::new(
        value
            .get("line")?
            .as_u64()
            .map(|line| u32::try_from(line).unwrap_or(u32::MAX))?,
        value
            .get("character")?
            .as_u64()
            .map(|character| u32::try_from(character).unwrap_or(u32::MAX))?,
    ))
}

fn lsp_position_for_char_offset(
    line_index: &LineIndex,
    offset: usize,
) -> editor_core_lsp::LspPosition {
    let (line, column) = line_index.char_offset_to_position(offset);
    let line_text = line_index.get_line_text(line).unwrap_or_default();
    editor_core_lsp::LspCoordinateConverter::position_to_lsp(&line_text, line, column)
}

fn word_at_char_column(line: &str, column: usize) -> Option<String> {
    let chars = line.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return None;
    }

    let mut idx = column.min(chars.len().saturating_sub(1));
    if !is_rename_word_char(chars[idx]) && idx > 0 && is_rename_word_char(chars[idx - 1]) {
        idx -= 1;
    }
    if !is_rename_word_char(chars[idx]) {
        return None;
    }

    let mut start = idx;
    while start > 0 && is_rename_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = idx + 1;
    while end < chars.len() && is_rename_word_char(chars[end]) {
        end += 1;
    }
    Some(chars[start..end].iter().collect())
}

fn is_rename_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn byte_index_for_char(value: &str, char_idx: usize) -> usize {
    value
        .char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(value.len())
}

fn insert_char_at(value: &mut String, char_idx: usize, ch: char) {
    let byte_idx = byte_index_for_char(value, char_idx);
    value.insert(byte_idx, ch);
}

fn remove_char_at(value: &mut String, char_idx: usize) {
    let start = byte_index_for_char(value, char_idx);
    if start >= value.len() {
        return;
    }
    let end = byte_index_for_char(value, char_idx.saturating_add(1));
    value.replace_range(start..end, "");
}
