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

#[derive(Clone)]
pub struct EditorWindowHandle {
    pub commands: EventQueue<EditorWindowCommand>,
    pub diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
}

pub struct EditorWindowView {
    _actions: EventQueue<AppAction>,
    commands: EventQueue<EditorWindowCommand>,

    editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: Binding<String>,
    diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    status: Binding<EditorStatus>,

    tab_window: atto_ui::composable::TabWindow,
    tabs: Vec<tabs::TabState>,
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
        Self {
            _actions: actions,
            commands,
            editor_theme,
            clipboard,
            diagnostics_summary,
            status,
            tab_window: atto_ui::composable::TabWindow::new(),
            tabs: Vec::new(),
        }
    }

    pub fn handle(
        commands: EventQueue<EditorWindowCommand>,
        diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary>,
    ) -> EditorWindowHandle {
        EditorWindowHandle {
            commands,
            diagnostics_summary,
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
}
