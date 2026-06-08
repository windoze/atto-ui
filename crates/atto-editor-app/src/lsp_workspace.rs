//! Workspace-scoped LSP session management for the editor app.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Duration;

use editor_core::{BufferId, LineIndex, Workspace, WorkspaceSymbol};
use editor_core_lsp::{ApplyWorkspaceEditResult, LspEvent, LspNotification, LspWorkspaceSync};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LspKey {
    pub workspace_root: PathBuf,
    pub language_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveWorkspaceDocument {
    pub key: LspKey,
    pub buffer_id: BufferId,
    pub uri: String,
}

#[derive(Clone, Debug)]
pub enum LspWorkspaceEvent {
    WorkspaceSymbols {
        query: String,
        symbols: Vec<WorkspaceSymbol>,
    },
    WorkspaceEditApplied {
        result: ApplyWorkspaceEditResult,
    },
    Message(String),
}

#[derive(Default)]
pub struct LspWorkspaceBridge {
    pub lsp_by_root_language: HashMap<LspKey, LspWorkspaceSync>,
    document_keys: HashMap<BufferId, LspKey>,
    pending_workspace_symbols: HashMap<(LspKey, u64), String>,
    active_document: Option<ActiveWorkspaceDocument>,
}

impl LspWorkspaceBridge {
    pub fn active_document(&self) -> Option<&ActiveWorkspaceDocument> {
        self.active_document.as_ref()
    }

    pub fn key_for_buffer(&self, buffer_id: BufferId) -> Option<&LspKey> {
        self.document_keys.get(&buffer_id)
    }

    pub fn open_document(
        &mut self,
        workspace: &Workspace,
        roots: &[PathBuf],
        path: &Path,
        buffer_id: BufferId,
        language_id: &str,
        lsp_mode: &atto_ui_editor::EditorLspMode,
    ) -> Result<(), String> {
        let key = LspKey {
            workspace_root: workspace_root_for_path(roots, path),
            language_id: language_id.to_string(),
        };
        self.document_keys.insert(buffer_id, key.clone());

        let atto_ui_editor::EditorLspMode::Enabled(cfg) = lsp_mode else {
            return Ok(());
        };
        if cfg.command.is_empty() {
            return Ok(());
        }

        if let Some(sync) = self.lsp_by_root_language.get_mut(&key) {
            sync.open_workspace_document(workspace, buffer_id, language_id.to_string())?;
            return Ok(());
        }

        let uri = editor_core_lsp::path_to_file_uri(path);
        let text = workspace
            .buffer_text(buffer_id)
            .map_err(|err| format!("Workspace buffer not found for LSP open: {err:?}"))?;
        let mut sync = LspWorkspaceSync::start(start_options_for_document(
            cfg,
            &key.workspace_root,
            uri,
            language_id.to_string(),
            text,
        )?)
        .map_err(|err| format!("Failed to start workspace LSP: {err}"))?;
        sync.set_auto_apply_workspace_edits(false);
        sync.session_mut().set_auto_refresh_options(
            editor_core_lsp::editor::LspAutoRefreshOptions {
                semantic_tokens: cfg.semantic_tokens,
                folding_ranges: cfg.folding_ranges,
                delay: Duration::from_millis(150),
            },
        );
        self.lsp_by_root_language.insert(key, sync);
        Ok(())
    }

    pub fn close_document(
        &mut self,
        workspace: &Workspace,
        buffer_id: BufferId,
    ) -> Result<(), String> {
        let Some(key) = self.document_keys.remove(&buffer_id) else {
            return Ok(());
        };
        if self
            .active_document
            .as_ref()
            .is_some_and(|doc| doc.buffer_id == buffer_id && doc.key == key)
        {
            self.active_document = None;
        }
        if let Some(sync) = self.lsp_by_root_language.get_mut(&key) {
            sync.close_workspace_document(workspace, buffer_id)?;
        }
        Ok(())
    }

    pub fn set_active_document(
        &mut self,
        workspace: &Workspace,
        buffer_id: BufferId,
    ) -> Result<(), String> {
        let Some(key) = self.document_keys.get(&buffer_id).cloned() else {
            return Ok(());
        };
        let uri = workspace
            .buffer_metadata(buffer_id)
            .and_then(|meta| meta.uri.clone())
            .ok_or_else(|| format!("Workspace buffer has no URI (id={})", buffer_id.get()))?;
        self.active_document = Some(ActiveWorkspaceDocument {
            key: key.clone(),
            buffer_id,
            uri,
        });
        if let Some(sync) = self.lsp_by_root_language.get_mut(&key) {
            sync.set_active_workspace_document(workspace, buffer_id)?;
        }
        Ok(())
    }

    pub fn did_change_from_text_delta(
        &mut self,
        workspace: &mut Workspace,
        buffer_id: BufferId,
    ) -> Result<(), String> {
        let Some(key) = self.document_keys.get(&buffer_id).cloned() else {
            return Ok(());
        };
        let Some(sync) = self.lsp_by_root_language.get_mut(&key) else {
            return Ok(());
        };
        sync.did_change_from_text_delta(workspace, buffer_id)
    }

    pub fn request_workspace_symbols(&mut self, query: String) -> Result<bool, String> {
        let Some(active) = self.active_document.as_ref() else {
            return Ok(false);
        };
        let Some(sync) = self.lsp_by_root_language.get_mut(&active.key) else {
            return Ok(false);
        };
        let id = sync.session_mut().request_workspace_symbol(query.clone())?;
        self.pending_workspace_symbols
            .insert((active.key.clone(), id), query);
        Ok(true)
    }

    pub fn apply_workspace_edit(
        &mut self,
        workspace: &mut Workspace,
        workspace_edit: &Value,
    ) -> Result<ApplyWorkspaceEditResult, String> {
        if let Some(active) = self.active_document.as_ref()
            && let Some(sync) = self.lsp_by_root_language.get_mut(&active.key)
        {
            return sync.apply_workspace_edit(workspace, workspace_edit);
        }
        editor_core_lsp::workspace_sync::apply_workspace_edit_to_workspace(
            workspace,
            workspace_edit,
        )
    }

    pub fn poll(&mut self, workspace: &mut Workspace) -> Vec<LspWorkspaceEvent> {
        let mut events = Vec::new();
        let active_key = self.active_poll_key(workspace);
        let keys = self
            .lsp_by_root_language
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            let Some(sync) = self.lsp_by_root_language.get_mut(&key) else {
                continue;
            };
            let drained = if active_key.as_ref() == Some(&key) {
                if let Err(err) = sync.poll_workspace(workspace) {
                    events.push(LspWorkspaceEvent::Message(format!(
                        "Workspace LSP poll failed: {err}"
                    )));
                    continue;
                }
                sync.drain_events()
            } else {
                let dummy = LineIndex::from_text("");
                if let Err(err) = sync
                    .session_mut()
                    .poll_edits_with_line_index_and_handler(&dummy, |_| {})
                {
                    events.push(LspWorkspaceEvent::Message(format!(
                        "Workspace LSP poll failed: {err}"
                    )));
                    continue;
                }
                sync.session_mut().drain_events()
            };

            for event in drained {
                match event {
                    LspEvent::Response(response) => {
                        if response.method == "workspace/symbol"
                            && let Some(query) = self
                                .pending_workspace_symbols
                                .remove(&(key.clone(), response.id))
                        {
                            let symbols = response
                                .result
                                .as_ref()
                                .map(editor_core_lsp::lsp_workspace_symbols_to_results)
                                .unwrap_or_default();
                            events.push(LspWorkspaceEvent::WorkspaceSymbols { query, symbols });
                        }
                    }
                    LspEvent::DeferredRequest(request)
                        if request.method == "workspace/applyEdit" =>
                    {
                        let workspace_edit =
                            request.params.get("edit").cloned().unwrap_or(Value::Null);
                        match sync.apply_workspace_edit(workspace, &workspace_edit) {
                            Ok(result) => {
                                let response =
                                    editor_core_lsp::workspace_sync::workspace_apply_edit_response(
                                        &result,
                                    );
                                if let Err(err) = sync
                                    .session_mut()
                                    .respond_to_server_request(request.id, response)
                                {
                                    events.push(LspWorkspaceEvent::Message(format!(
                                        "Workspace LSP applyEdit response failed: {err}"
                                    )));
                                }
                                events.push(LspWorkspaceEvent::WorkspaceEditApplied { result });
                            }
                            Err(err) => {
                                let response = json!({
                                    "applied": false,
                                    "failureReason": err,
                                });
                                let _ = sync
                                    .session_mut()
                                    .respond_to_server_request(request.id, response);
                                events.push(LspWorkspaceEvent::Message(
                                    "Workspace LSP applyEdit failed".to_string(),
                                ));
                            }
                        }
                    }
                    LspEvent::Notification(notification) => {
                        if let Some(message) = notification_message(notification) {
                            events.push(LspWorkspaceEvent::Message(message));
                        }
                    }
                    LspEvent::DeferredRequest(_) => {}
                }
            }
        }
        events
    }

    fn active_poll_key(&self, workspace: &Workspace) -> Option<LspKey> {
        workspace
            .active_buffer_id()
            .and_then(|buffer_id| self.document_keys.get(&buffer_id).cloned())
    }
}

fn start_options_for_document(
    cfg: &atto_ui_editor::EditorLspConfig,
    root: &Path,
    document_uri: String,
    language_id: String,
    initial_text: String,
) -> Result<editor_core_lsp::LspSessionStartOptions, String> {
    let program = cfg
        .command
        .first()
        .cloned()
        .ok_or_else(|| "Workspace LSP command is empty".to_string())?;
    let mut cmd = ProcessCommand::new(program);
    cmd.args(cfg.command.iter().skip(1));
    cmd.stderr(std::process::Stdio::null());

    let root_uri = editor_core_lsp::path_to_file_uri(root);
    let workspace_folders = vec![json!({ "uri": root_uri, "name": root.to_string_lossy() })];
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

    let initialize_params = json!({
        "processId": std::process::id(),
        "rootUri": root_uri,
        "workspaceFolders": workspace_folders.clone(),
        "capabilities": {
            "workspace": {
                "applyEdit": true,
                "configuration": true,
                "workspaceFolders": true,
                "symbol": { "dynamicRegistration": false },
            },
            "textDocument": {
                "documentSymbol": { "dynamicRegistration": false },
                "rename": { "dynamicRegistration": false, "prepareSupport": true },
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
            },
        },
        "clientInfo": { "name": "atto-ui editor workspace" },
    });

    Ok(editor_core_lsp::LspSessionStartOptions {
        cmd,
        workspace_folders,
        initialize_params,
        initialize_timeout: cfg.initialize_timeout,
        document: editor_core_lsp::LspDocument {
            uri: document_uri,
            language_id,
            version: 1,
        },
        initial_text,
    })
}

fn workspace_root_for_path(roots: &[PathBuf], path: &Path) -> PathBuf {
    roots
        .iter()
        .filter(|root| path.starts_with(root))
        .max_by_key(|root| root.components().count())
        .cloned()
        .or_else(|| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn notification_message(notification: LspNotification) -> Option<String> {
    match notification {
        LspNotification::ShowMessage(message) => Some(message.message),
        LspNotification::LogMessage(message) => Some(message.message),
        LspNotification::Progress(_)
        | LspNotification::Telemetry(_)
        | LspNotification::PublishDiagnostics(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_root_uses_longest_matching_root() {
        let roots = vec![PathBuf::from("/tmp/ws"), PathBuf::from("/tmp/ws/crate")];
        let path = PathBuf::from("/tmp/ws/crate/src/lib.rs");

        assert_eq!(workspace_root_for_path(&roots, &path), roots[1]);
    }

    #[test]
    fn active_poll_key_uses_key_for_workspace_active_buffer() {
        let mut workspace = Workspace::new();
        let first = workspace
            .open_buffer(None, "one\n", 80)
            .expect("open first");
        let second = workspace
            .open_buffer(None, "two\n", 80)
            .expect("open second");
        workspace
            .set_active_view(second.view_id)
            .expect("activate second");

        let first_key = LspKey {
            workspace_root: PathBuf::from("/tmp/one"),
            language_id: "rust".to_string(),
        };
        let second_key = LspKey {
            workspace_root: PathBuf::from("/tmp/two"),
            language_id: "python".to_string(),
        };
        let mut bridge = LspWorkspaceBridge::default();
        bridge.document_keys.insert(first.buffer_id, first_key);
        bridge
            .document_keys
            .insert(second.buffer_id, second_key.clone());

        assert_eq!(bridge.active_poll_key(&workspace), Some(second_key));
    }
}
