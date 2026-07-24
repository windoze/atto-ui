//! File-backed tab state and editor-window command handling.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use atto_ui::reactive::{Binding, DirtyObserver, EventQueue};
use editor_core::{BufferId, TextEditSpec};

use crate::language::{guess_language_id, lsp_mode_for_file, syntax_config_for_file};
use crate::workspace_state::TabRef;

use super::document_tab::{DocumentTabView, SaveSettingsBindings, TabCommand};
use super::{EditorStatus, EditorWindowCommand, EditorWindowView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteEdit {
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PendingSaveAfterFormat {
    Save,
    SaveAs(PathBuf),
}

pub(super) struct TabState {
    pub(super) tab_id: u64,
    pub(super) path: Option<PathBuf>,
    pub(super) title_base: String,
    language_id: String,
    text: Binding<String>,
    pub(super) format_on_save: Binding<bool>,
    pub(super) trim_trailing_whitespace_on_save: Binding<bool>,
    last_saved_text: String,
    text_observer: DirtyObserver,
    pub(super) is_dirty: bool,
    diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    pub(super) events: EventQueue<atto_ui_editor::EditorEvent>,
    commands: EventQueue<TabCommand>,
    workspace_buffer_id: Option<BufferId>,
    workspace_tab: Option<TabRef>,
    pub(super) pending_save_after_format: Option<PendingSaveAfterFormat>,
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

        // Only a genuinely missing file opens as an empty buffer (new file). Any other read error
        // — permission denied, non-UTF-8, transient IO — must NOT open a fake-empty document: it
        // would present as clean and a later Save would `fs::write` the empty text back, silently
        // truncating the real file on disk.
        let disk_text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                self.workspace_state
                    .lock()
                    .record_error(format!("无法打开文件 {}: {err}", path.display()));
                return;
            }
        };
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
        let format_on_save: Binding<bool> = false.into();
        let trim_trailing_whitespace_on_save: Binding<bool> = false.into();
        let tab_commands: EventQueue<TabCommand> = EventQueue::new();

        let (tab_view, tab_handle) = DocumentTabView::new(
            tab_commands.clone(),
            self.editor_theme.clone(),
            self.clipboard.clone(),
            text.clone(),
            SaveSettingsBindings {
                format_on_save: format_on_save.clone(),
                trim_trailing_whitespace_on_save: trim_trailing_whitespace_on_save.clone(),
            },
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
            format_on_save,
            trim_trailing_whitespace_on_save,
            last_saved_text: initial_text,
            text_observer,
            is_dirty: false,
            diagnostics_summary: tab_handle.diagnostics_summary.clone(),
            events: tab_handle.events.clone(),
            commands: tab_commands.clone(),
            workspace_buffer_id,
            workspace_tab,
            pending_save_after_format: None,
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

    pub(super) fn save_tab_at(&mut self, active: usize) -> Result<()> {
        if self
            .tabs
            .get(active)
            .and_then(|tab| tab.path.as_ref())
            .is_none()
        {
            return Ok(());
        }
        self.apply_save_transforms(active)?;

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

    fn format_active(&mut self, save_after: bool) {
        let pending_save = save_after.then_some(PendingSaveAfterFormat::Save);
        self.format_active_with_pending_save(pending_save);
    }

    fn format_active_with_pending_save(&mut self, pending_save: Option<PendingSaveAfterFormat>) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        let save_after = pending_save.is_some();
        if let Some(pending_save) = pending_save
            && let Some(tab) = self.tabs.get_mut(active)
        {
            tab.pending_save_after_format = Some(pending_save);
        }
        self.send_tab_command_to_active(TabCommand::FormatDocument { save_after });
    }

    fn save_active_with_format_on_save(&mut self) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        let format_on_save = self
            .tabs
            .get(active)
            .is_some_and(|tab| tab.format_on_save.get());
        if format_on_save {
            self.format_active_with_pending_save(Some(PendingSaveAfterFormat::Save));
            return;
        }
        if let Err(err) = self.save_tab_at(active) {
            self.events.push(atto_ui_editor::EditorEvent::LspMessage {
                message: format!("Save failed: {err:#}"),
            });
        }
    }

    fn save_as_active_with_format_on_save(&mut self, path: PathBuf) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        let path = Self::canonicalize_best_effort(&path);
        let format_on_save = self
            .tabs
            .get(active)
            .is_some_and(|tab| tab.format_on_save.get());
        if format_on_save {
            self.format_active_with_pending_save(Some(PendingSaveAfterFormat::SaveAs(path)));
            return;
        }
        if let Err(err) = self.save_as_tab_at(active, path) {
            self.events.push(atto_ui_editor::EditorEvent::LspMessage {
                message: format!("Save As failed: {err:#}"),
            });
        }
    }

    pub(super) fn save_as_tab_at(&mut self, active: usize, path: PathBuf) -> Result<()> {
        let path = Self::canonicalize_best_effort(&path);
        {
            let Some(tab) = self.tabs.get_mut(active) else {
                return Ok(());
            };

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
        }

        self.apply_save_transforms(active)?;

        let Some(tab) = self.tabs.get_mut(active) else {
            return Ok(());
        };
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

    fn apply_save_transforms(&mut self, active: usize) -> Result<()> {
        let workspace_state = self.workspace_state.clone();
        let Some(tab) = self.tabs.get_mut(active) else {
            return Ok(());
        };
        trim_tab_trailing_whitespace(tab, &workspace_state)
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
                    self.save_active_with_format_on_save();
                }
                EditorWindowCommand::SaveAs(path) => {
                    self.save_as_active_with_format_on_save(path);
                }
                EditorWindowCommand::FormatActive => {
                    self.format_active(false);
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

fn trim_tab_trailing_whitespace(
    tab: &mut TabState,
    workspace_state: &crate::workspace_state::SharedWorkspaceState,
) -> Result<()> {
    if !tab.trim_trailing_whitespace_on_save.get() {
        return Ok(());
    }

    if let Some(buffer_id) = tab.workspace_buffer_id {
        let mut workspace = workspace_state.lock();
        if let Some(tab_ref) = tab.workspace_tab
            && let Err(err) = workspace.sync_tab_to_buffer(tab_ref)
        {
            return Err(anyhow!(err));
        }

        let text = workspace
            .buffer_text(buffer_id)
            .map_err(anyhow::Error::msg)?;
        let edits = trailing_whitespace_text_edits(&text);
        workspace
            .apply_text_edits_to_buffer(buffer_id, edits)
            .map_err(anyhow::Error::msg)?;
        return Ok(());
    }

    let text = tab.text.get();
    let trimmed = apply_byte_edits(&text, &trailing_whitespace_byte_edits(&text));
    if trimmed != text {
        tab.text.set(trimmed);
    }
    Ok(())
}

fn trailing_whitespace_text_edits(text: &str) -> Vec<TextEditSpec> {
    trailing_whitespace_byte_edits(text)
        .into_iter()
        .map(|edit| TextEditSpec {
            start: char_offset_at(text, edit.start),
            end: char_offset_at(text, edit.end),
            text: String::new(),
        })
        .collect()
}

fn trailing_whitespace_byte_edits(text: &str) -> Vec<ByteEdit> {
    let bytes = text.as_bytes();
    let mut edits = Vec::new();
    let mut line_start = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                push_trailing_whitespace_edit(bytes, line_start, i, &mut edits);
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    i += 2;
                } else {
                    i += 1;
                }
                line_start = i;
            }
            b'\n' => {
                push_trailing_whitespace_edit(bytes, line_start, i, &mut edits);
                i += 1;
                line_start = i;
            }
            _ => i += 1,
        }
    }

    push_trailing_whitespace_edit(bytes, line_start, bytes.len(), &mut edits);
    edits
}

fn push_trailing_whitespace_edit(
    bytes: &[u8],
    line_start: usize,
    line_end: usize,
    edits: &mut Vec<ByteEdit>,
) {
    let mut trim_start = line_end;
    while trim_start > line_start && matches!(bytes[trim_start - 1], b' ' | b'\t') {
        trim_start -= 1;
    }
    if trim_start < line_end {
        edits.push(ByteEdit {
            start: trim_start,
            end: line_end,
        });
    }
}

fn char_offset_at(text: &str, byte_index: usize) -> usize {
    text[..byte_index].chars().count()
}

fn apply_byte_edits(text: &str, edits: &[ByteEdit]) -> String {
    if edits.is_empty() {
        return text.to_string();
    }

    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for edit in edits {
        output.push_str(&text[cursor..edit.start]);
        cursor = edit.end;
    }
    output.push_str(&text[cursor..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use atto_ui::wm::WindowId;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("atto_editor_tabs_{prefix}_{nanos}"))
    }

    fn test_window() -> EditorWindowView {
        let mut window = EditorWindowView::new(
            EventQueue::<crate::actions::AppAction>::new(),
            EventQueue::<EditorWindowCommand>::new(),
            atto_ui_editor::EditorThemeSet::default().into(),
            String::new().into(),
            atto_ui_editor::DiagnosticsSummary::default().into(),
        );
        window.set_window_id(WindowId::from_raw(1));
        window
    }

    fn open_test_file(prefix: &str, text: &str) -> (PathBuf, EditorWindowView) {
        let root = unique_temp_dir(prefix);
        fs::create_dir_all(&root).expect("create temp root");
        let path = root.join("main.rs");
        fs::write(&path, text).expect("write file");

        let mut window = test_window();
        window.open_file_in_tab(path.clone());
        assert_eq!(window.tabs.len(), 1);
        (path, window)
    }

    #[test]
    fn trailing_whitespace_edits_preserve_content_and_final_newline_state() {
        let text = "alpha beta  \n\tindented value\t \nno-newline  ";
        let trimmed = apply_byte_edits(text, &trailing_whitespace_byte_edits(text));
        assert_eq!(trimmed, "alpha beta\n\tindented value\nno-newline");
    }

    #[test]
    fn save_keeps_trailing_whitespace_by_default() {
        let (path, mut window) = open_test_file("trim_default", "fn main() {}\n");
        window.tabs[0]
            .text
            .set("let value = alpha + beta;   \n".to_string());
        window.update_tab_titles();

        window.save_tab_at(0).expect("save tab");

        assert_eq!(
            fs::read_to_string(path).expect("read saved file"),
            "let value = alpha + beta;   \n"
        );
        assert!(!window.tabs[0].is_dirty);
    }

    #[test]
    fn save_trims_trailing_whitespace_when_enabled_and_clears_dirty() {
        let (path, mut window) = open_test_file("trim_enabled", "fn main() {}\n");
        window.tabs[0].trim_trailing_whitespace_on_save.set(true);
        window.tabs[0]
            .text
            .set("let value = alpha + beta;   \nlast\t".to_string());
        window.update_tab_titles();
        assert!(window.tabs[0].is_dirty);

        window.save_tab_at(0).expect("save tab");

        assert_eq!(
            fs::read_to_string(path).expect("read saved file"),
            "let value = alpha + beta;\nlast"
        );
        assert_eq!(window.tabs[0].text.get(), "let value = alpha + beta;\nlast");
        assert_eq!(
            window.tabs[0].last_saved_text,
            "let value = alpha + beta;\nlast"
        );
        assert!(!window.tabs[0].is_dirty);
    }

    #[test]
    fn format_on_save_completion_trims_before_writing() {
        let (path, mut window) = open_test_file("trim_after_format", "fn main() {}\n");
        window.tabs[0].trim_trailing_whitespace_on_save.set(true);
        window.tabs[0].text.set("formatted line   \n".to_string());
        window.update_tab_titles();
        window.tabs[0].pending_save_after_format = Some(PendingSaveAfterFormat::Save);
        window.tabs[0]
            .events
            .push(atto_ui_editor::EditorEvent::FormatFinished {
                success: true,
                changed: true,
            });

        window.sync_editor_events();

        assert_eq!(
            fs::read_to_string(path).expect("read saved file"),
            "formatted line\n"
        );
        assert!(window.tabs[0].pending_save_after_format.is_none());
        assert!(!window.tabs[0].is_dirty);
    }

    #[test]
    fn save_as_with_format_on_save_waits_for_format_then_trims_to_target() {
        let (source_path, mut window) = open_test_file("save_as_format", "fn main() {}\n");
        let target_path = source_path.with_file_name("formatted.rs");
        let canonical_target = EditorWindowView::canonicalize_best_effort(&target_path);
        window.tabs[0].format_on_save.set(true);
        window.tabs[0].trim_trailing_whitespace_on_save.set(true);
        window.tabs[0].text.set("before format   \n".to_string());
        window.update_tab_titles();

        window
            .commands
            .push(EditorWindowCommand::SaveAs(target_path.clone()));
        window.handle_commands();

        assert_eq!(
            window.tabs[0].pending_save_after_format.as_ref(),
            Some(&PendingSaveAfterFormat::SaveAs(canonical_target.clone()))
        );
        assert!(!target_path.exists());

        window.tabs[0].text.set("formatted output   \n".to_string());
        window.tabs[0]
            .events
            .push(atto_ui_editor::EditorEvent::FormatFinished {
                success: true,
                changed: true,
            });

        window.sync_editor_events();

        assert_eq!(
            fs::read_to_string(&target_path).expect("read save-as target"),
            "formatted output\n"
        );
        assert_eq!(window.tabs[0].path.as_ref(), Some(&canonical_target));
        assert_eq!(window.tabs[0].last_saved_text, "formatted output\n");
        assert!(window.tabs[0].pending_save_after_format.is_none());
        assert!(!window.tabs[0].is_dirty);
    }

    #[test]
    fn failed_save_keeps_dirty_marker() {
        let (path, mut window) = open_test_file("save_failure", "fn main() {}\n");
        let failure_path = path.with_file_name("write_target_directory");
        fs::create_dir_all(&failure_path).expect("create directory save target");
        window.tabs[0].path = Some(failure_path);
        window.tabs[0].trim_trailing_whitespace_on_save.set(true);
        window.tabs[0].text.set("changed line   \n".to_string());
        window.update_tab_titles();
        assert!(window.tabs[0].is_dirty);

        let result = window.save_tab_at(0);

        assert!(result.is_err());
        assert_eq!(window.tabs[0].text.get(), "changed line\n");
        assert_eq!(window.tabs[0].last_saved_text, "fn main() {}\n");
        assert!(window.tabs[0].is_dirty);
    }

    #[test]
    fn save_trims_crlf_file_without_changing_line_endings_or_final_newline() {
        let (path, mut window) = open_test_file("trim_crlf", "alpha  \r\nbeta\t\r\ngamma  ");
        window.tabs[0].trim_trailing_whitespace_on_save.set(true);

        window.save_tab_at(0).expect("save tab");

        assert_eq!(
            fs::read(path).expect("read saved file"),
            b"alpha\r\nbeta\r\ngamma"
        );
        assert_eq!(window.tabs[0].text.get(), "alpha\nbeta\ngamma");
        assert!(!window.tabs[0].is_dirty);
    }

    #[test]
    fn opening_missing_file_creates_one_empty_new_file_tab() {
        let root = unique_temp_dir("missing");
        fs::create_dir_all(&root).expect("create temp root");
        let path = root.join("brand_new.rs");
        assert!(!path.exists());

        let mut window = test_window();
        window.open_file_in_tab(path.clone());

        // A genuinely missing file is a valid "new file": one empty tab, no error.
        assert_eq!(window.tabs.len(), 1);
        assert_eq!(window.tabs[0].text.get(), "");
        assert!(window.workspace_state.lock().take_last_error().is_none());
    }

    #[test]
    fn opening_unreadable_file_does_not_open_a_truncating_tab() {
        // A non-UTF-8 file exists on disk but `read_to_string` rejects it (InvalidData). The old
        // `unwrap_or_default()` would open it as an empty, clean buffer; a later Save would then
        // `fs::write` the empty text back and destroy the file. Assert we refuse to open it and
        // surface an error instead — so the bytes on disk are never at risk.
        let root = unique_temp_dir("unreadable");
        fs::create_dir_all(&root).expect("create temp root");
        let path = root.join("binary.bin");
        let original = [0xff, 0xfe, b'd', b'a', b't', b'a'];
        fs::write(&path, original).expect("write non-utf8 file");

        let mut window = test_window();
        window.open_file_in_tab(path.clone());

        assert_eq!(window.tabs.len(), 0, "unreadable file must not open a tab");
        assert!(
            window.workspace_state.lock().take_last_error().is_some(),
            "read failure should surface an error"
        );
        // The original bytes are untouched.
        assert_eq!(fs::read(&path).expect("re-read file"), original);
    }
}
