//! File-backed tab state and editor-window command handling.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use atto_ui::reactive::{Binding, DirtyObserver, EventQueue};
use editor_core::BufferId;

use crate::language::{guess_language_id, lsp_mode_for_file, syntax_config_for_file};
use crate::workspace_state::TabRef;

use super::document_tab::{DocumentTabView, TabCommand};
use super::{EditorStatus, EditorWindowCommand, EditorWindowView};

pub(super) struct TabState {
    pub(super) tab_id: u64,
    pub(super) path: Option<PathBuf>,
    pub(super) title_base: String,
    language_id: String,
    text: Binding<String>,
    last_saved_text: String,
    text_observer: DirtyObserver,
    pub(super) is_dirty: bool,
    diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    pub(super) events: EventQueue<atto_ui_editor::EditorEvent>,
    commands: EventQueue<TabCommand>,
    workspace_buffer_id: Option<BufferId>,
    workspace_tab: Option<TabRef>,
}

impl EditorWindowView {
    fn canonicalize_best_effort(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn open_file_in_tab(&mut self, path: PathBuf) {
        let path = Self::canonicalize_best_effort(&path);

        if let Some((idx, _)) = self
            .tabs
            .iter()
            .enumerate()
            .find(|(_i, tab)| tab.path.as_ref().is_some_and(|p| p == &path))
        {
            let _ = self.tab_window.select_tab(idx);
            self.sync_active_workspace_document();
            return;
        }

        let disk_text = std::fs::read_to_string(&path).unwrap_or_default();
        let title_base = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<file>")
            .to_string();

        let language_id = guess_language_id(&path);
        let syntax = syntax_config_for_file(&path, &language_id);
        let lsp = lsp_mode_for_file(&path, &language_id);
        let tab_id = self.next_tab_id;
        let workspace_tab = self.tab_ref(tab_id);
        let workspace_open = if workspace_tab.is_some() {
            let mut workspace = self.workspace_state.lock();
            match workspace.prepare_file_tab(&path, &disk_text, 80, &language_id, &lsp) {
                Ok(opened) => Some(opened),
                Err(err) => {
                    workspace.record_error(err);
                    return;
                }
            }
        } else {
            None
        };
        let initial_text = workspace_open
            .as_ref()
            .map(|opened| opened.text.clone())
            .unwrap_or(disk_text);

        let text: Binding<String> = initial_text.clone().into();
        let tab_commands: EventQueue<TabCommand> = EventQueue::new();

        let (tab_view, tab_handle) = DocumentTabView::new(
            tab_commands.clone(),
            self.editor_theme.clone(),
            self.clipboard.clone(),
            text.clone(),
            language_id.clone(),
            syntax,
            lsp,
        );

        let idx = self
            .tab_window
            .add_tab(title_base.clone(), Box::new(tab_view));
        let _ = self.tab_window.select_tab(idx);

        let mut text_observer = text.dirty_observer();
        text.check_dirty(&mut text_observer);

        self.next_tab_id += 1;
        let workspace_buffer_id = workspace_open.as_ref().map(|opened| opened.buffer_id);
        self.tabs.push(TabState {
            tab_id,
            path: Some(path.clone()),
            title_base,
            language_id,
            text: text.clone(),
            last_saved_text: initial_text,
            text_observer,
            is_dirty: false,
            diagnostics_summary: tab_handle.diagnostics_summary.clone(),
            events: tab_handle.events.clone(),
            commands: tab_commands.clone(),
            workspace_buffer_id,
            workspace_tab,
        });

        if let (Some(tab_ref), Some(opened)) = (workspace_tab, workspace_open.as_ref()) {
            let mut workspace = self.workspace_state.lock();
            if let Err(err) = workspace.register_tab_binding(
                tab_ref,
                opened.buffer_id,
                opened.view_id,
                text.clone(),
            ) {
                workspace.record_error(err);
            }
        }
    }

    fn select_tab_by_id(&mut self, tab_id: u64) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.tab_id == tab_id) {
            let _ = self.tab_window.select_tab(index);
            self.sync_active_workspace_document();
        }
    }

    fn close_active_tab(&mut self) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        if self.tab_window.remove_tab(active).is_some() && active < self.tabs.len() {
            let tab = self.tabs.remove(active);
            if let Some(tab_ref) = tab.workspace_tab {
                let mut workspace = self.workspace_state.lock();
                if let Err(err) = workspace.unregister_tab(tab_ref) {
                    workspace.record_error(err);
                }
            }
            self.sync_active_workspace_document();
        }
    }

    fn send_tab_command_to_active(&mut self, cmd: TabCommand) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        if let Some(tab) = self.tabs.get(active) {
            tab.commands.push(cmd);
        }
    }

    fn jump_active_tab_to(&mut self, target: crate::actions::JumpTarget) {
        self.send_tab_command_to_active(TabCommand::JumpTo(target));
    }

    fn save_active(&mut self) -> Result<()> {
        let Some(active) = self.tab_window.active_tab() else {
            return Ok(());
        };
        let Some(tab) = self.tabs.get_mut(active) else {
            return Ok(());
        };
        let Some(path) = tab.path.clone() else {
            return Ok(());
        };

        let save_text = if let Some(buffer_id) = tab.workspace_buffer_id {
            let mut workspace = self.workspace_state.lock();
            if let Some(tab_ref) = tab.workspace_tab
                && let Err(err) = workspace.sync_tab_to_buffer(tab_ref)
            {
                return Err(anyhow!(err));
            }
            workspace
                .buffer_text_for_saving(buffer_id)
                .map_err(|err| anyhow!(err))?
        } else {
            tab.text.get()
        };

        std::fs::write(&path, save_text)?;
        if let Some(buffer_id) = tab.workspace_buffer_id {
            let mut workspace = self.workspace_state.lock();
            workspace
                .mark_buffer_saved(buffer_id)
                .map_err(|err| anyhow!(err))?;
        }
        let current_text = tab.text.get();
        tab.last_saved_text = current_text;
        tab.is_dirty = false;
        self.tab_window
            .set_tab_title(active, tab.title_base.clone());
        Ok(())
    }

    fn save_as_active(&mut self, path: PathBuf) -> Result<()> {
        let Some(active) = self.tab_window.active_tab() else {
            return Ok(());
        };
        let Some(tab) = self.tabs.get_mut(active) else {
            return Ok(());
        };

        let path = Self::canonicalize_best_effort(&path);
        if let Some(buffer_id) = tab.workspace_buffer_id {
            let mut workspace = self.workspace_state.lock();
            if let Some(tab_ref) = tab.workspace_tab
                && let Err(err) = workspace.sync_tab_to_buffer(tab_ref)
            {
                return Err(anyhow!(err));
            }
            let lsp = lsp_mode_for_file(&path, &tab.language_id);
            workspace
                .set_buffer_path(buffer_id, &path, &tab.language_id, &lsp)
                .map_err(|err| anyhow!(err))?;
        }
        let save_text = if let Some(buffer_id) = tab.workspace_buffer_id {
            let workspace = self.workspace_state.lock();
            workspace
                .buffer_text_for_saving(buffer_id)
                .map_err(|err| anyhow!(err))?
        } else {
            tab.text.get()
        };

        std::fs::write(&path, save_text)?;
        if let Some(buffer_id) = tab.workspace_buffer_id {
            let mut workspace = self.workspace_state.lock();
            workspace
                .mark_buffer_saved(buffer_id)
                .map_err(|err| anyhow!(err))?;
        }
        tab.path = Some(path.clone());
        tab.title_base = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<file>")
            .to_string();
        let current_text = tab.text.get();
        tab.last_saved_text = current_text;
        tab.is_dirty = false;
        self.tab_window
            .set_tab_title(active, tab.title_base.clone());
        Ok(())
    }

    pub(super) fn update_tab_titles(&mut self) {
        for (idx, tab) in self.tabs.iter_mut().enumerate() {
            if !tab.text.check_dirty(&mut tab.text_observer) {
                continue;
            }
            if let Some(buffer_id) = tab.workspace_buffer_id {
                let current_text = tab.text.get();
                let mut workspace = self.workspace_state.lock();
                if let Err(err) = workspace.sync_buffer_text(buffer_id, &current_text) {
                    workspace.record_error(err);
                }
            }
            tab.is_dirty = tab.text.get() != tab.last_saved_text;
            let title = if tab.is_dirty {
                format!("{}*", tab.title_base)
            } else {
                tab.title_base.clone()
            };
            self.tab_window.set_tab_title(idx, title);
        }
    }

    pub(super) fn active_diagnostics_summary(&self) -> atto_ui_editor::DiagnosticsSummary {
        self.tab_window
            .active_tab()
            .and_then(|idx| self.tabs.get(idx))
            .map(|tab| tab.diagnostics_summary.get())
            .unwrap_or_default()
    }

    pub(super) fn active_status(&self) -> EditorStatus {
        self.tab_window
            .active_tab()
            .and_then(|idx| self.tabs.get(idx))
            .map(|tab| EditorStatus {
                path: tab.path.clone(),
                language: tab.language_id.clone(),
                dirty: tab.is_dirty,
            })
            .unwrap_or_default()
    }

    pub(super) fn handle_commands(&mut self) {
        for cmd in self.commands.drain() {
            match cmd {
                EditorWindowCommand::OpenFile(path) => self.open_file_in_tab(path),
                EditorWindowCommand::OpenFileAndJump { path, target } => {
                    self.open_file_in_tab(path);
                    self.jump_active_tab_to(target);
                }
                EditorWindowCommand::SelectTabById(tab_id) => self.select_tab_by_id(tab_id),
                EditorWindowCommand::JumpTo(target) => self.jump_active_tab_to(target),
                EditorWindowCommand::RequestDocumentSymbols => {
                    self.send_tab_command_to_active(TabCommand::RequestDocumentSymbols)
                }
                EditorWindowCommand::RequestWorkspaceSymbols(query) => {
                    self.send_tab_command_to_active(TabCommand::RequestWorkspaceSymbols(query))
                }
                EditorWindowCommand::SaveActive => {
                    let _ = self.save_active();
                }
                EditorWindowCommand::SaveAs(path) => {
                    let _ = self.save_as_active(path);
                }
                EditorWindowCommand::CloseActiveTab => self.close_active_tab(),
                EditorWindowCommand::SplitVertical => {
                    self.send_tab_command_to_active(TabCommand::SplitVertical)
                }
                EditorWindowCommand::SplitHorizontal => {
                    self.send_tab_command_to_active(TabCommand::SplitHorizontal)
                }
                EditorWindowCommand::CloseSplit => {
                    self.send_tab_command_to_active(TabCommand::CloseSplit)
                }
                EditorWindowCommand::EditorAction(action) => {
                    self.send_tab_command_to_active(TabCommand::EditorAction(action))
                }
            }
        }
    }
}
