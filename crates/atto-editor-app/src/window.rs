//! Editor window composition and tab management.

use std::path::PathBuf;

use atto_ui::reactive::{Binding, EventQueue};

use crate::actions::AppAction;

mod component_impl;
mod document_tab;
mod tabs;
mod util;

#[derive(Clone, Debug)]
pub enum EditorWindowCommand {
    OpenFile(PathBuf),
    SelectTabById(u64),

    SaveActive,
    SaveAs(PathBuf),
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
    pub diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    pub tab_summaries: Binding<Vec<EditorTabSummary>>,
}

pub struct EditorWindowView {
    _actions: EventQueue<AppAction>,
    commands: EventQueue<EditorWindowCommand>,

    editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: Binding<String>,
    diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    status: Binding<EditorStatus>,
    tab_summaries: Binding<Vec<EditorTabSummary>>,

    tab_window: atto_ui::composable::TabWindow,
    tabs: Vec<tabs::TabState>,
    next_tab_id: u64,
}

impl EditorWindowView {
    pub fn new(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    ) -> Self {
        Self::new_with_status(
            actions,
            commands,
            editor_theme,
            clipboard,
            diagnostics_summary,
            EditorStatus::default().into(),
        )
    }

    pub fn new_with_status(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
        status: Binding<EditorStatus>,
    ) -> Self {
        Self::new_with_status_and_tabs(
            actions,
            commands,
            editor_theme,
            clipboard,
            diagnostics_summary,
            status,
            Vec::<EditorTabSummary>::new().into(),
        )
    }

    pub fn new_with_status_and_tabs(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
        status: Binding<EditorStatus>,
        tab_summaries: Binding<Vec<EditorTabSummary>>,
    ) -> Self {
        Self {
            _actions: actions,
            commands,
            editor_theme,
            clipboard,
            diagnostics_summary,
            status,
            tab_summaries,
            tab_window: atto_ui::composable::TabWindow::new(),
            tabs: Vec::new(),
            next_tab_id: 1,
        }
    }

    pub fn handle(
        commands: EventQueue<EditorWindowCommand>,
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    ) -> EditorWindowHandle {
        EditorWindowHandle {
            commands,
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
}
