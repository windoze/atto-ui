//! Editor window composition and tab management.

use std::path::PathBuf;

use atto_ui::reactive::{Binding, EventQueue};
use atto_ui::wm::WindowId;

use crate::actions::{AppAction, JumpTarget};
use crate::workspace_state::{SharedWorkspaceState, TabRef, WorkspaceState};

mod component_impl;
mod document_tab;
mod tabs;
mod util;

use tabs::PendingSaveAfterFormat;

#[derive(Clone, Debug)]
pub enum EditorWindowCommand {
    OpenFile(PathBuf),
    OpenFileAndJump { path: PathBuf, target: JumpTarget },
    SelectTabById(u64),
    JumpTo(JumpTarget),
    RequestDocumentSymbols,
    RequestWorkspaceSymbols(String),

    SaveActive,
    SaveAs(PathBuf),
    FormatActive,
    CloseActiveTab,

    SplitVertical,
    SplitHorizontal,
    CloseSplit,

    EditorAction(atto_ui_editor::EditorAction),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorStatus {
    pub path: Option<PathBuf>,
    pub language: String,
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorTabSummary {
    pub tab_id: u64,
    pub title: String,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub active: bool,
}

#[derive(Clone)]
pub struct EditorWindowHandle {
    pub commands: EventQueue<EditorWindowCommand>,
    pub events: EventQueue<atto_ui_editor::EditorEvent>,
    pub diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    pub tab_summaries: Binding<Vec<EditorTabSummary>>,
}

#[derive(Clone)]
pub struct EditorWindowBindings {
    pub diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    pub status: Binding<EditorStatus>,
    pub tab_summaries: Binding<Vec<EditorTabSummary>>,
}

impl EditorWindowBindings {
    pub fn new(
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
        status: Binding<EditorStatus>,
        tab_summaries: Binding<Vec<EditorTabSummary>>,
    ) -> Self {
        Self {
            diagnostics_summary,
            status,
            tab_summaries,
        }
    }

    pub fn with_status(
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
        status: Binding<EditorStatus>,
    ) -> Self {
        Self::new(
            diagnostics_summary,
            status,
            Vec::<EditorTabSummary>::new().into(),
        )
    }
}

pub struct EditorWindowView {
    _actions: EventQueue<AppAction>,
    commands: EventQueue<EditorWindowCommand>,
    events: EventQueue<atto_ui_editor::EditorEvent>,

    editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: Binding<String>,
    diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    status: Binding<EditorStatus>,
    tab_summaries: Binding<Vec<EditorTabSummary>>,

    tab_window: atto_ui::composable::TabWindow,
    tabs: Vec<tabs::TabState>,
    next_tab_id: u64,
    window_id: Option<WindowId>,
    workspace_state: SharedWorkspaceState,
}

impl EditorWindowView {
    pub fn new(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    ) -> Self {
        let events = EventQueue::new();
        Self::new_with_status(
            actions,
            commands,
            events,
            editor_theme,
            clipboard,
            diagnostics_summary,
            EditorStatus::default().into(),
        )
    }

    pub fn new_with_status(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        events: EventQueue<atto_ui_editor::EditorEvent>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
        status: Binding<EditorStatus>,
    ) -> Self {
        Self::new_with_bindings(
            actions,
            commands,
            events,
            editor_theme,
            clipboard,
            EditorWindowBindings::with_status(diagnostics_summary, status),
        )
    }

    pub fn new_with_bindings(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        events: EventQueue<atto_ui_editor::EditorEvent>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        bindings: EditorWindowBindings,
    ) -> Self {
        Self::new_with_workspace_bindings(
            actions,
            commands,
            events,
            editor_theme,
            clipboard,
            WorkspaceState::shared(),
            bindings,
        )
    }

    pub fn new_with_workspace_bindings(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        events: EventQueue<atto_ui_editor::EditorEvent>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        workspace_state: SharedWorkspaceState,
        bindings: EditorWindowBindings,
    ) -> Self {
        Self {
            _actions: actions,
            commands,
            events,
            editor_theme,
            clipboard,
            diagnostics_summary: bindings.diagnostics_summary,
            status: bindings.status,
            tab_summaries: bindings.tab_summaries,
            tab_window: atto_ui::composable::TabWindow::new(),
            tabs: Vec::new(),
            next_tab_id: 1,
            window_id: None,
            workspace_state,
        }
    }

    pub fn handle(
        commands: EventQueue<EditorWindowCommand>,
        events: EventQueue<atto_ui_editor::EditorEvent>,
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    ) -> EditorWindowHandle {
        EditorWindowHandle {
            commands,
            events,
            diagnostics_summary,
            tab_summaries: Vec::<EditorTabSummary>::new().into(),
        }
    }

    pub(super) fn sync_active_diagnostics_summary(&self) {
        let summary = self.active_diagnostics_summary();
        if self.diagnostics_summary.get() != summary {
            self.diagnostics_summary.set(summary);
        }
    }

    pub(super) fn sync_active_status(&self) {
        let status = self.active_status();
        if self.status.get() != status {
            self.status.set(status);
        }
    }

    pub fn tab_summaries(&self) -> Vec<EditorTabSummary> {
        let active = self.tab_window.active_tab();
        self.tabs
            .iter()
            .enumerate()
            .map(|(idx, tab)| EditorTabSummary {
                tab_id: tab.tab_id,
                title: tab.title_base.clone(),
                path: tab.path.clone(),
                dirty: tab.is_dirty,
                active: active == Some(idx),
            })
            .collect()
    }

    pub(super) fn sync_tab_summaries(&self) {
        let summaries = self.tab_summaries();
        if self.tab_summaries.get() != summaries {
            self.tab_summaries.set(summaries);
        }
    }

    pub(super) fn sync_editor_events(&mut self) {
        let mut save_after_format = Vec::new();

        for idx in 0..self.tabs.len() {
            for event in self.tabs[idx].events.drain() {
                if let atto_ui_editor::EditorEvent::FormatFinished {
                    success,
                    changed: _,
                } = &event
                    && let Some(pending_save) = self.tabs[idx].pending_save_after_format.take()
                {
                    if *success {
                        save_after_format.push((idx, pending_save));
                    } else {
                        self.events.push(atto_ui_editor::EditorEvent::LspMessage {
                            message: "Format-on-save failed; save skipped".to_string(),
                        });
                    }
                }
                self.events.push(event);
            }
        }

        for (idx, pending_save) in save_after_format {
            match pending_save {
                PendingSaveAfterFormat::Save => {
                    if let Err(err) = self.save_tab_at(idx) {
                        self.events.push(atto_ui_editor::EditorEvent::LspMessage {
                            message: format!("Save failed: {err:#}"),
                        });
                    }
                }
                PendingSaveAfterFormat::SaveAs(path) => {
                    if let Err(err) = self.save_as_tab_at(idx, path) {
                        self.events.push(atto_ui_editor::EditorEvent::LspMessage {
                            message: format!("Save As failed: {err:#}"),
                        });
                    }
                }
            }
        }
    }

    pub(super) fn set_window_id(&mut self, window_id: WindowId) {
        self.window_id = Some(window_id);
    }

    pub(super) fn tab_ref(&self, tab_id: u64) -> Option<TabRef> {
        self.window_id
            .map(|window_id| TabRef::new(window_id, tab_id))
    }

    pub(super) fn sync_active_workspace_document(&self) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        let Some(tab) = self.tabs.get(active) else {
            return;
        };
        let Some(tab_ref) = self.tab_ref(tab.tab_id) else {
            return;
        };
        let mut workspace = self.workspace_state.lock();
        if let Err(err) = workspace.set_active_tab(tab_ref) {
            workspace.record_error(err);
        }
    }
}
