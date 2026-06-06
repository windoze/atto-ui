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
}

#[derive(Clone)]
pub struct EditorWindowHandle {
    pub commands: EventQueue<EditorWindowCommand>,
}

pub struct EditorWindowView {
    _actions: EventQueue<AppAction>,
    commands: EventQueue<EditorWindowCommand>,

    editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: Binding<String>,

    tab_window: atto_ui::composable::TabWindow,
    tabs: Vec<tabs::TabState>,
}

impl EditorWindowView {
    pub fn new(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
    ) -> Self {
        Self {
            _actions: actions,
            commands,
            editor_theme,
            clipboard,
            tab_window: atto_ui::composable::TabWindow::new(),
            tabs: Vec::new(),
        }
    }

    pub fn handle(commands: EventQueue<EditorWindowCommand>) -> EditorWindowHandle {
        EditorWindowHandle { commands }
    }
}
