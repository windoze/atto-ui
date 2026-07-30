//! Top-level menu bar construction ([`build_menu`]).

use super::*;


pub(crate) fn build_menu(actions: EventQueue<AppAction>) -> MenuBar {
    MenuBar::new(vec![
        MenuSpec::new(
            "&File",
            vec![
                MenuItem::action("Open File…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenFileDialog(OpenTarget::NewTab))
                })
                .accelerator("Ctrl+O"),
                MenuItem::action("Open File (New Window)…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenFileDialog(OpenTarget::NewWindow))
                }),
                MenuItem::action("Open Folder…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenFolderDialog)
                }),
                MenuItem::action("Quick Open…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenFilePicker)
                })
                .accelerator("Ctrl+P"),
                MenuItem::action("Save", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Save)
                })
                .accelerator("Ctrl+S"),
                MenuItem::action("Save As…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::SaveAsDialog)
                }),
                MenuItem::action("Quit", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Quit)
                })
                .accelerator("Ctrl+Q"),
            ],
        ),
        MenuSpec::new(
            "&View",
            vec![
                MenuItem::action("Toggle Explorer Window", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::ToggleExplorer)
                })
                .accelerator("Ctrl+E"),
                MenuItem::action("Dock Explorer Left", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::ExplorerLeft)
                }),
                MenuItem::action("Dock Explorer Right", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::ExplorerRight)
                }),
            ],
        ),
        MenuSpec::new(
            "&Navigate",
            vec![
                MenuItem::action("Document Symbols…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenDocumentSymbolPicker)
                }),
                MenuItem::action("Workspace Symbols…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenWorkspaceSymbolPicker)
                }),
                MenuItem::action("Global Search…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenGlobalSearch)
                })
                .accelerator("Ctrl+Shift+F"),
            ],
        ),
        MenuSpec::new(
            "&Split",
            vec![
                MenuItem::action("Split Vertical", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::SplitVertical)
                }),
                MenuItem::action("Split Horizontal", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::SplitHorizontal)
                }),
                MenuItem::action("Close Split", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::CloseSplit)
                }),
            ],
        ),
    ])
}
