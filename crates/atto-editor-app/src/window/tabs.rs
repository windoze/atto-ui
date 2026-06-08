//! File-backed tab state and editor-window command handling.

use std::path::{Path, PathBuf};

use anyhow::Result;
use atto_ui::reactive::{Binding, DirtyObserver, EventQueue};

use crate::language::{guess_language_id, lsp_mode_for_file, syntax_config_for_file};

use super::document_tab::{DocumentTabView, TabCommand};
use super::{EditorStatus, EditorWindowCommand, EditorWindowView};

pub(super) struct TabState {
    path: Option<PathBuf>,
    title_base: String,
    language_id: String,
    text: Binding<String>,
    last_saved_text: String,
    text_observer: DirtyObserver,
    is_dirty: bool,
    diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    commands: EventQueue<TabCommand>,
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
            return;
        }

        let initial_text = std::fs::read_to_string(&path).unwrap_or_default();
        let title_base = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<file>")
            .to_string();

        let text: Binding<String> = initial_text.clone().into();
        let tab_commands: EventQueue<TabCommand> = EventQueue::new();

        let language_id = guess_language_id(&path);
        let syntax = syntax_config_for_file(&path, &language_id);
        let lsp = lsp_mode_for_file(&path, &language_id);

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

        self.tabs.push(TabState {
            path: Some(path.clone()),
            title_base,
            language_id,
            text: text.clone(),
            last_saved_text: initial_text,
            text_observer,
            is_dirty: false,
            diagnostics_summary: tab_handle.diagnostics_summary.clone(),
            commands: tab_commands.clone(),
        });
    }

    fn close_active_tab(&mut self) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        if self.tab_window.remove_tab(active).is_some() && active < self.tabs.len() {
            self.tabs.remove(active);
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

        std::fs::write(&path, tab.text.get())?;
        tab.last_saved_text = tab.text.get();
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

        std::fs::write(&path, tab.text.get())?;
        tab.path = Some(Self::canonicalize_best_effort(&path));
        tab.title_base = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<file>")
            .to_string();
        tab.last_saved_text = tab.text.get();
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
            }
        }
    }
}
