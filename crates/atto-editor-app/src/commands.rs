use atto_ui::app::{CommandDescriptor, CommandRegistry, KeyChord, KeySequence};
use crossterm::event::{KeyCode, KeyModifiers};

use crate::actions::{AppAction, OpenTarget};
use crate::window::EditorWindowCommand;

/// Action payload used by the app-level command registry.
#[derive(Clone, Debug)]
pub enum AppCommandAction {
    App(AppAction),
    EditorWindow(EditorWindowCommand),
    Editor(atto_ui_editor::EditorAction),
    OpenCommandPalette,
}

/// Creates the app command registry shared by keymaps and future command pickers.
pub fn app_command_registry() -> CommandRegistry<AppCommandAction> {
    CommandRegistry::new(vec![
        command(
            "file.open",
            "Open File",
            "File",
            AppCommandAction::App(AppAction::OpenFileDialog(OpenTarget::NewTab)),
        )
        .with_default_sequence(prefixed('o')),
        command(
            "file.openNewWindow",
            "Open File in New Window",
            "File",
            AppCommandAction::App(AppAction::OpenFileDialog(OpenTarget::NewWindow)),
        ),
        command(
            "file.openFolder",
            "Open Folder",
            "File",
            AppCommandAction::App(AppAction::OpenFolderDialog),
        )
        .with_default_sequence(prefixed('d')),
        command(
            "picker.file",
            "File Picker",
            "Picker",
            AppCommandAction::App(AppAction::OpenFilePicker),
        )
        .with_default_sequence(ctrl('p')),
        command(
            "file.save",
            "Save",
            "File",
            AppCommandAction::App(AppAction::Save),
        )
        .with_default_sequence(prefixed('a')),
        command(
            "file.saveAs",
            "Save As",
            "File",
            AppCommandAction::App(AppAction::SaveAsDialog),
        )
        .with_default_sequence(prefixed('p')),
        command(
            "file.quit",
            "Quit",
            "File",
            AppCommandAction::App(AppAction::Quit),
        )
        .with_default_sequence(prefixed('x')),
        command(
            "view.toggleExplorer",
            "Toggle Explorer",
            "View",
            AppCommandAction::App(AppAction::ToggleExplorer),
        )
        .with_default_sequence(prefixed('e')),
        command(
            "view.explorerLeft",
            "Dock Explorer Left",
            "View",
            AppCommandAction::App(AppAction::ExplorerLeft),
        )
        .with_default_sequence(prefixed('l')),
        command(
            "view.explorerRight",
            "Dock Explorer Right",
            "View",
            AppCommandAction::App(AppAction::ExplorerRight),
        )
        .with_default_sequence(prefixed('r')),
        command(
            "split.closeTab",
            "Close Tab",
            "Split",
            AppCommandAction::EditorWindow(EditorWindowCommand::CloseActiveTab),
        )
        .with_default_sequence(prefixed('t')),
        command(
            "split.vertical",
            "Split Vertical",
            "Split",
            AppCommandAction::EditorWindow(EditorWindowCommand::SplitVertical),
        )
        .with_default_sequence(prefixed('b')),
        command(
            "split.horizontal",
            "Split Horizontal",
            "Split",
            AppCommandAction::EditorWindow(EditorWindowCommand::SplitHorizontal),
        )
        .with_default_sequence(prefixed('h')),
        command(
            "split.close",
            "Close Split",
            "Split",
            AppCommandAction::EditorWindow(EditorWindowCommand::CloseSplit),
        )
        .with_default_sequence(prefixed('c')),
        command(
            "editor.undo",
            "Undo",
            "Editor",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::Undo),
        )
        .with_default_sequence(prefixed('u')),
        command(
            "editor.redo",
            "Redo",
            "Editor",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::Redo),
        )
        .with_default_sequence(prefixed('y')),
        command(
            "editor.find",
            "Find",
            "Editor",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::Find),
        ),
        command(
            "editor.replace",
            "Replace",
            "Editor",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::Replace),
        ),
        command(
            "editor.toggleComment",
            "Toggle Comment",
            "Editor",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::ToggleComment),
        )
        .with_default_sequence(prefixed('/')),
        command(
            "editor.toggleLineNumbers",
            "Toggle Line Numbers",
            "Editor",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::ToggleLineNumbers),
        ),
        command(
            "editor.toggleFoldingMarkers",
            "Toggle Folding Markers",
            "Editor",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::ToggleFoldingMarkers),
        ),
        command(
            "lsp.hover",
            "LSP Hover",
            "LSP",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::LspRequestHover),
        ),
        command(
            "lsp.completion",
            "LSP Completion",
            "LSP",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::LspRequestCompletion),
        ),
        command(
            "lsp.gotoDefinition",
            "Go to Definition",
            "LSP",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::LspGotoDefinition),
        )
        .with_default_sequence(prefixed('g')),
        command(
            "lsp.gotoReferences",
            "Go to References",
            "LSP",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::LspGotoReferences),
        ),
        command(
            "lsp.nextDiagnostic",
            "Next Diagnostic",
            "LSP",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::LspNextDiagnostic),
        ),
        command(
            "lsp.prevDiagnostic",
            "Previous Diagnostic",
            "LSP",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::LspPrevDiagnostic),
        ),
        command(
            "lsp.codeAction",
            "Code Action",
            "LSP",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::LspCodeAction),
        )
        .with_default_sequence(prefixed('.')),
        command(
            "lsp.rename",
            "Rename Symbol",
            "LSP",
            AppCommandAction::Editor(atto_ui_editor::EditorAction::LspRename),
        ),
        command(
            "picker.commandPalette",
            "Command Palette",
            "Picker",
            AppCommandAction::OpenCommandPalette,
        )
        .with_default_sequence(ctrl_shift('p')),
        command(
            "picker.buffer",
            "Buffer Picker",
            "Picker",
            AppCommandAction::App(AppAction::OpenBufferPicker),
        ),
        command(
            "picker.documentSymbols",
            "Document Symbols",
            "Picker",
            AppCommandAction::App(AppAction::OpenDocumentSymbolPicker),
        )
        .with_default_sequence(prefixed('s')),
        command(
            "picker.workspaceSymbols",
            "Workspace Symbols",
            "Picker",
            AppCommandAction::App(AppAction::OpenWorkspaceSymbolPicker),
        )
        .with_default_sequence(prefixed('w')),
        command(
            "search.global",
            "Global Search",
            "Search",
            AppCommandAction::App(AppAction::OpenGlobalSearch),
        )
        .with_default_sequence(ctrl_shift('f')),
    ])
    .expect("app command ids must be unique")
}

pub fn command_prefix() -> KeyChord {
    ctrl_alt('k')
}

fn command<T: Into<AppCommandAction>>(
    id: &str,
    title: &str,
    category: &str,
    action: T,
) -> CommandDescriptor<AppCommandAction> {
    CommandDescriptor::new(id, title, category, action.into())
}

fn prefixed(ch: char) -> KeySequence {
    KeySequence::new(vec![command_prefix(), ctrl_alt(ch)])
}

fn ctrl_alt(ch: char) -> KeyChord {
    KeyChord::new(KeyCode::Char(ch), KeyModifiers::CONTROL | KeyModifiers::ALT)
}

fn ctrl(ch: char) -> KeyChord {
    KeyChord::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
}

fn ctrl_shift(ch: char) -> KeyChord {
    KeyChord::new(
        KeyCode::Char(ch),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_command_registry_has_unique_ids_and_core_commands() {
        let registry = app_command_registry();

        assert!(registry.get("file.save").is_some());
        assert!(registry.get("view.toggleExplorer").is_some());
        assert!(registry.get("split.vertical").is_some());
        assert!(registry.get("editor.toggleComment").is_some());
        assert!(registry.get("lsp.codeAction").is_some());
        assert!(registry.get("picker.file").is_some());
        assert!(registry.get("picker.commandPalette").is_some());
        assert!(registry.get("picker.buffer").is_some());
        assert!(registry.get("picker.documentSymbols").is_some());
        assert!(registry.get("picker.workspaceSymbols").is_some());
        assert!(registry.get("search.global").is_some());
    }
}
