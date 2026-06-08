//! Shared editor workspace state used to bridge existing tab bindings to `editor-core`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use atto_ui::reactive::Binding;
use atto_ui::wm::WindowId;
use editor_core::{BufferId, TextEditSpec, ViewId, Workspace};
use editor_core_lsp::ApplyWorkspaceEditResult;
use parking_lot::Mutex;
use serde_json::Value;

use crate::lsp_workspace::{LspWorkspaceBridge, LspWorkspaceEvent};

pub type SharedWorkspaceState = Arc<Mutex<WorkspaceState>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabRef {
    pub window_id: WindowId,
    pub tab_id: u64,
}

impl TabRef {
    pub fn new(window_id: WindowId, tab_id: u64) -> Self {
        Self { window_id, tab_id }
    }
}

#[derive(Clone, Debug)]
pub struct WorkspaceOpenResult {
    pub buffer_id: BufferId,
    pub view_id: ViewId,
    pub text: String,
    pub reused_buffer: bool,
}

pub struct WorkspaceState {
    pub workspace: Workspace,
    pub path_to_buffer: HashMap<PathBuf, BufferId>,
    pub buffer_to_tabs: HashMap<BufferId, Vec<TabRef>>,
    pub lsp: LspWorkspaceBridge,
    workspace_roots: Vec<PathBuf>,
    buffer_to_path: HashMap<BufferId, PathBuf>,
    tab_to_buffer: HashMap<TabRef, BufferId>,
    tab_to_view: HashMap<TabRef, ViewId>,
    tab_text_bindings: HashMap<TabRef, Binding<String>>,
    last_error: Option<String>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            workspace: Workspace::new(),
            path_to_buffer: HashMap::new(),
            buffer_to_tabs: HashMap::new(),
            lsp: LspWorkspaceBridge::default(),
            workspace_roots: Vec::new(),
            buffer_to_path: HashMap::new(),
            tab_to_buffer: HashMap::new(),
            tab_to_view: HashMap::new(),
            tab_text_bindings: HashMap::new(),
            last_error: None,
        }
    }
}

impl WorkspaceState {
    pub fn shared() -> SharedWorkspaceState {
        Arc::new(Mutex::new(Self::default()))
    }

    pub fn set_workspace_roots(&mut self, roots: Vec<PathBuf>) {
        self.workspace_roots = roots;
    }

    pub fn workspace_roots(&self) -> &[PathBuf] {
        &self.workspace_roots
    }

    pub fn record_error(&mut self, error: impl Into<String>) {
        self.last_error = Some(error.into());
    }

    pub fn take_last_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    pub fn prepare_file_tab(
        &mut self,
        path: &Path,
        initial_text: &str,
        viewport_width: usize,
        language_id: &str,
        lsp_mode: &atto_ui_editor::EditorLspMode,
    ) -> Result<WorkspaceOpenResult, String> {
        let path = path.to_path_buf();
        if let Some(buffer_id) = self.path_to_buffer.get(&path).copied() {
            let view_id = self
                .workspace
                .create_view(buffer_id, viewport_width.max(1))
                .map_err(|err| format!("Workspace create_view failed: {err:?}"))?;
            let text = self
                .workspace
                .buffer_text(buffer_id)
                .map_err(|err| format!("Workspace buffer_text failed: {err:?}"))?;
            self.open_lsp_document(&path, buffer_id, language_id, lsp_mode);
            return Ok(WorkspaceOpenResult {
                buffer_id,
                view_id,
                text,
                reused_buffer: true,
            });
        }

        let uri = editor_core_lsp::path_to_file_uri(&path);
        let opened = self
            .workspace
            .open_buffer(Some(uri), initial_text, viewport_width.max(1))
            .map_err(|err| format!("Workspace open_buffer failed: {err:?}"))?;
        self.path_to_buffer.insert(path.clone(), opened.buffer_id);
        self.buffer_to_path.insert(opened.buffer_id, path.clone());
        self.open_lsp_document(&path, opened.buffer_id, language_id, lsp_mode);
        Ok(WorkspaceOpenResult {
            buffer_id: opened.buffer_id,
            view_id: opened.view_id,
            text: initial_text.to_string(),
            reused_buffer: false,
        })
    }

    pub fn register_tab_binding(
        &mut self,
        tab: TabRef,
        buffer_id: BufferId,
        view_id: ViewId,
        text: Binding<String>,
    ) -> Result<(), String> {
        self.tab_to_buffer.insert(tab, buffer_id);
        self.tab_to_view.insert(tab, view_id);
        self.tab_text_bindings.insert(tab, text);
        let tabs = self.buffer_to_tabs.entry(buffer_id).or_default();
        if !tabs.contains(&tab) {
            tabs.push(tab);
        }
        self.set_active_tab(tab)
    }

    pub fn unregister_tab(&mut self, tab: TabRef) -> Result<(), String> {
        let Some(buffer_id) = self.tab_to_buffer.remove(&tab) else {
            return Ok(());
        };
        let view_id = self.tab_to_view.remove(&tab);
        self.tab_text_bindings.remove(&tab);

        let remaining_tabs = if let Some(tabs) = self.buffer_to_tabs.get_mut(&buffer_id) {
            tabs.retain(|existing| *existing != tab);
            tabs.len()
        } else {
            0
        };

        if remaining_tabs == 0 {
            let _ = self.lsp.close_document(&self.workspace, buffer_id);
            if let Some(path) = self.buffer_to_path.remove(&buffer_id) {
                self.path_to_buffer.remove(&path);
            }
            self.buffer_to_tabs.remove(&buffer_id);
            self.workspace
                .close_buffer(buffer_id)
                .map_err(|err| format!("Workspace close_buffer failed: {err:?}"))?;
        } else if let Some(view_id) = view_id {
            self.workspace
                .close_view(view_id)
                .map_err(|err| format!("Workspace close_view failed: {err:?}"))?;
        }

        Ok(())
    }

    pub fn unregister_window(&mut self, window_id: WindowId) -> Result<(), String> {
        let tabs = self
            .tab_to_buffer
            .keys()
            .copied()
            .filter(|tab| tab.window_id == window_id)
            .collect::<Vec<_>>();
        for tab in tabs {
            self.unregister_tab(tab)?;
        }
        Ok(())
    }

    pub fn set_active_tab(&mut self, tab: TabRef) -> Result<(), String> {
        let Some(view_id) = self.tab_to_view.get(&tab).copied() else {
            return Ok(());
        };
        self.workspace
            .set_active_view(view_id)
            .map_err(|err| format!("Workspace set_active_view failed: {err:?}"))?;
        let Some(buffer_id) = self.tab_to_buffer.get(&tab).copied() else {
            return Ok(());
        };
        self.lsp.set_active_document(&self.workspace, buffer_id)
    }

    pub fn active_buffer_id(&self) -> Option<BufferId> {
        self.workspace.active_buffer_id()
    }

    pub fn buffer_id_for_tab(&self, tab: TabRef) -> Option<BufferId> {
        self.tab_to_buffer.get(&tab).copied()
    }

    pub fn sync_tab_to_buffer(&mut self, tab: TabRef) -> Result<bool, String> {
        let Some(buffer_id) = self.tab_to_buffer.get(&tab).copied() else {
            return Ok(false);
        };
        let Some(text) = self.tab_text_bindings.get(&tab).map(Binding::get) else {
            return Ok(false);
        };
        self.sync_buffer_text(buffer_id, &text)
    }

    pub fn sync_buffer_text(&mut self, buffer_id: BufferId, text: &str) -> Result<bool, String> {
        let current = self
            .workspace
            .buffer_text(buffer_id)
            .map_err(|err| format!("Workspace buffer_text failed: {err:?}"))?;
        if current == text {
            return Ok(false);
        }

        let end = self
            .workspace
            .buffer_char_count(buffer_id)
            .map_err(|err| format!("Workspace buffer_char_count failed: {err:?}"))?;
        self.workspace
            .apply_text_edits(vec![(
                buffer_id,
                vec![TextEditSpec {
                    start: 0,
                    end,
                    text: text.to_string(),
                }],
            )])
            .map_err(|err| format!("Workspace apply_text_edits failed: {err:?}"))?;
        self.lsp
            .did_change_from_text_delta(&mut self.workspace, buffer_id)?;
        Ok(true)
    }

    pub fn buffer_text_for_saving(&self, buffer_id: BufferId) -> Result<String, String> {
        self.workspace
            .buffer_text_for_saving(buffer_id)
            .map_err(|err| format!("Workspace buffer_text_for_saving failed: {err:?}"))
    }

    pub fn mark_buffer_saved(&mut self, buffer_id: BufferId) -> Result<(), String> {
        self.workspace
            .mark_saved_for_buffer(buffer_id)
            .map_err(|err| format!("Workspace mark_saved_for_buffer failed: {err:?}"))
    }

    pub fn set_buffer_path(
        &mut self,
        buffer_id: BufferId,
        path: &Path,
        language_id: &str,
        lsp_mode: &atto_ui_editor::EditorLspMode,
    ) -> Result<(), String> {
        let path = path.to_path_buf();
        if self
            .path_to_buffer
            .get(&path)
            .is_some_and(|existing| *existing != buffer_id)
        {
            return Err(format!(
                "Path is already open in another buffer: {}",
                path.display()
            ));
        }

        let _ = self.lsp.close_document(&self.workspace, buffer_id);
        if let Some(old_path) = self.buffer_to_path.insert(buffer_id, path.clone()) {
            self.path_to_buffer.remove(&old_path);
        }
        self.path_to_buffer.insert(path.clone(), buffer_id);
        self.workspace
            .set_buffer_uri(buffer_id, Some(editor_core_lsp::path_to_file_uri(&path)))
            .map_err(|err| format!("Workspace set_buffer_uri failed: {err:?}"))?;
        self.open_lsp_document(&path, buffer_id, language_id, lsp_mode);
        Ok(())
    }

    pub fn apply_workspace_edit(
        &mut self,
        workspace_edit: &Value,
    ) -> Result<ApplyWorkspaceEditResult, String> {
        let result = self
            .lsp
            .apply_workspace_edit(&mut self.workspace, workspace_edit)?;
        self.sync_applied_workspace_edit(&result)?;
        Ok(result)
    }

    pub fn request_workspace_symbols(&mut self, query: String) -> Result<bool, String> {
        self.lsp.request_workspace_symbols(query)
    }

    pub fn poll_lsp(&mut self) -> Vec<LspWorkspaceEvent> {
        let events = self.lsp.poll(&mut self.workspace);
        for event in &events {
            if let LspWorkspaceEvent::WorkspaceEditApplied { result } = event
                && let Err(err) = self.sync_applied_workspace_edit(result)
            {
                self.record_error(err);
            }
        }
        events
    }

    pub fn sync_buffer_to_tabs(&self, buffer_id: BufferId) -> Result<usize, String> {
        let text = self
            .workspace
            .buffer_text(buffer_id)
            .map_err(|err| format!("Workspace buffer_text failed: {err:?}"))?;
        let mut updated = 0usize;
        if let Some(tabs) = self.buffer_to_tabs.get(&buffer_id) {
            for tab in tabs {
                if let Some(binding) = self.tab_text_bindings.get(tab)
                    && binding.get() != text
                {
                    binding.set(text.clone());
                    updated = updated.saturating_add(1);
                }
            }
        }
        Ok(updated)
    }

    fn sync_applied_workspace_edit(&self, result: &ApplyWorkspaceEditResult) -> Result<(), String> {
        for document in &result.applied {
            if let Some(buffer_id) = self.workspace.buffer_id_for_uri(&document.uri) {
                self.sync_buffer_to_tabs(buffer_id)?;
            }
        }
        Ok(())
    }

    fn open_lsp_document(
        &mut self,
        path: &Path,
        buffer_id: BufferId,
        language_id: &str,
        lsp_mode: &atto_ui_editor::EditorLspMode,
    ) {
        if let Err(err) = self.lsp.open_document(
            &self.workspace,
            &self.workspace_roots,
            path,
            buffer_id,
            language_id,
            lsp_mode,
        ) {
            self.record_error(err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("atto_editor_workspace_state_{prefix}_{nanos}"))
    }

    fn open_test_tab(
        state: &mut WorkspaceState,
        path: &Path,
        text: &str,
        tab: TabRef,
    ) -> Binding<String> {
        let opened = state
            .prepare_file_tab(
                path,
                text,
                80,
                "plaintext",
                &atto_ui_editor::EditorLspMode::Disabled,
            )
            .expect("workspace open");
        let binding: Binding<String> = opened.text.clone().into();
        state
            .register_tab_binding(tab, opened.buffer_id, opened.view_id, binding.clone())
            .expect("register tab");
        binding
    }

    #[test]
    fn repeated_file_open_reuses_workspace_buffer() {
        let root = unique_temp_dir("reuse");
        fs::create_dir_all(&root).expect("create temp root");
        let path = root.join("main.rs");
        fs::write(&path, "fn main() {}\n").expect("write file");

        let mut state = WorkspaceState::default();
        state.set_workspace_roots(vec![root.clone()]);

        let first = state
            .prepare_file_tab(
                &path,
                "fn main() {}\n",
                80,
                "rust",
                &atto_ui_editor::EditorLspMode::Disabled,
            )
            .expect("first open");
        state
            .register_tab_binding(
                TabRef::new(WindowId::from_raw(1), 1),
                first.buffer_id,
                first.view_id,
                first.text.clone().into(),
            )
            .expect("register first");

        let second = state
            .prepare_file_tab(
                &path,
                "stale disk text",
                80,
                "rust",
                &atto_ui_editor::EditorLspMode::Disabled,
            )
            .expect("second open");

        assert!(second.reused_buffer);
        assert_eq!(second.buffer_id, first.buffer_id);
        assert_eq!(state.workspace.len(), 1);
        assert_eq!(state.workspace.view_count(), 2);
        assert_eq!(second.text, "fn main() {}\n");
    }

    #[test]
    fn workspace_edit_updates_registered_tab_bindings() {
        let root = unique_temp_dir("workspace_edit");
        fs::create_dir_all(&root).expect("create temp root");
        let first = root.join("one.txt");
        let second = root.join("two.txt");

        let mut state = WorkspaceState::default();
        state.set_workspace_roots(vec![root.clone()]);
        let first_binding = open_test_tab(
            &mut state,
            &first,
            "hello world\n",
            TabRef::new(WindowId::from_raw(1), 1),
        );
        let second_binding = open_test_tab(
            &mut state,
            &second,
            "goodbye world\n",
            TabRef::new(WindowId::from_raw(1), 2),
        );

        let mut changes = serde_json::Map::new();
        changes.insert(
            editor_core_lsp::path_to_file_uri(&first),
            json!([
                {
                    "range": {
                        "start": { "line": 0, "character": 6 },
                        "end": { "line": 0, "character": 11 }
                    },
                    "newText": "atto"
                }
            ]),
        );
        changes.insert(
            editor_core_lsp::path_to_file_uri(&second),
            json!([
                {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 7 }
                    },
                    "newText": "hello"
                }
            ]),
        );
        let edit = json!({ "changes": changes });

        let result = state.apply_workspace_edit(&edit).expect("apply edit");

        assert_eq!(result.applied.len(), 2);
        assert_eq!(first_binding.get(), "hello atto\n");
        assert_eq!(second_binding.get(), "hello world\n");
    }

    #[test]
    fn active_tab_switch_updates_workspace_active_buffer() {
        let root = unique_temp_dir("active");
        fs::create_dir_all(&root).expect("create temp root");
        let first = root.join("one.txt");
        let second = root.join("two.txt");

        let mut state = WorkspaceState::default();
        state.set_workspace_roots(vec![root.clone()]);
        open_test_tab(
            &mut state,
            &first,
            "one\n",
            TabRef::new(WindowId::from_raw(1), 1),
        );
        open_test_tab(
            &mut state,
            &second,
            "two\n",
            TabRef::new(WindowId::from_raw(1), 2),
        );

        let first_buffer = state
            .buffer_id_for_tab(TabRef::new(WindowId::from_raw(1), 1))
            .expect("first buffer");
        let second_buffer = state
            .buffer_id_for_tab(TabRef::new(WindowId::from_raw(1), 2))
            .expect("second buffer");

        state
            .set_active_tab(TabRef::new(WindowId::from_raw(1), 1))
            .expect("activate first");
        assert_eq!(state.active_buffer_id(), Some(first_buffer));

        state
            .set_active_tab(TabRef::new(WindowId::from_raw(1), 2))
            .expect("activate second");
        assert_eq!(state.active_buffer_id(), Some(second_buffer));
        assert_eq!(
            state.lsp.active_document().map(|doc| doc.buffer_id),
            Some(second_buffer)
        );
    }
}
