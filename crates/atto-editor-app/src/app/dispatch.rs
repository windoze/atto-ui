//! Action dispatch: [`handle_action`] (the `AppAction` dispatcher), the
//! command-palette key-event router, and `AppCommandAction` execution.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_action(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
    action: AppAction,
    open_file_result: &Property<Option<PathBuf>>,
    save_as_result: &Property<Option<PathBuf>>,
    open_folder_input: &Property<String>,
    editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: atto_ui::reactive::Binding<String>,
) -> Result<AppControl> {
    match action {
        AppAction::Quit => return Ok(AppControl::Exit),

        AppAction::OpenFileDialog(target) => {
            if desktop.wm.has_active_modal() {
                return Ok(AppControl::Continue);
            }
            state.lock().open_file_target = Some(target);
            open_file_result.set(None);
            let work = Desktop::layout(screen).work_area;
            let rect = centered_rect(work, 76, 22);
            desktop.add_window(
                Window::new(
                    WindowKind::Modal,
                    "Open File",
                    rect,
                    Box::new(FileDialog::open_file(open_file_result.binding())),
                ),
                screen,
            );
        }

        AppAction::Save => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::SaveActive);
            }
        }
        AppAction::SaveAsDialog => {
            if desktop.wm.has_active_modal() {
                return Ok(AppControl::Continue);
            }
            save_as_result.set(None);
            let work = Desktop::layout(screen).work_area;
            let rect = centered_rect(work, 76, 22);
            desktop.add_window(
                Window::new(
                    WindowKind::Modal,
                    "Save As",
                    rect,
                    Box::new(FileDialog::save_file(save_as_result.binding())),
                ),
                screen,
            );
        }
        AppAction::OpenCommandPalette => {
            open_command_palette(desktop, screen, state);
        }
        AppAction::OpenFilePicker => {
            open_file_picker(desktop, screen, state);
        }
        AppAction::OpenBufferPicker => {
            open_buffer_picker(desktop, screen, state);
        }
        AppAction::OpenDocumentSymbolPicker => {
            request_document_symbols(desktop, state);
        }
        AppAction::OpenWorkspaceSymbolPicker => {
            open_workspace_symbol_query_picker(desktop, screen, state);
        }
        AppAction::OpenGlobalSearch => {
            open_global_search_query_picker(desktop, screen, state);
        }

        AppAction::CloseTab => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::CloseActiveTab);
            }
        }
        AppAction::SplitVertical => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::SplitVertical);
            }
        }
        AppAction::SplitHorizontal => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::SplitHorizontal);
            }
        }
        AppAction::CloseSplit => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::CloseSplit);
            }
        }

        AppAction::ToggleExplorer => {
            toggle_explorer_window(desktop, screen, state, actions.clone());
        }
        AppAction::ExplorerLeft => {
            dock_explorer_window(desktop, screen, state, DockSide::Left);
        }
        AppAction::ExplorerRight => {
            dock_explorer_window(desktop, screen, state, DockSide::Right);
        }

        AppAction::OpenFolderDialog => {
            if desktop.wm.has_active_modal() {
                return Ok(AppControl::Continue);
            }

            let initial = env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .to_string_lossy()
                .to_string();
            open_folder_input.set(initial);

            let work = Desktop::layout(screen).work_area;
            let rect = centered_rect(work, 72, 9);
            let view = build_open_folder_view(open_folder_input.clone(), actions.clone());
            let id = desktop.add_window(
                Window::new(WindowKind::Modal, "Open Folder", rect, view),
                screen,
            );
            state.lock().open_folder_modal = Some(id);
        }

        AppAction::SubmitOpenFolderDialog => {
            let Some(modal) = state.lock().open_folder_modal.take() else {
                return Ok(AppControl::Continue);
            };
            desktop.wm.close(modal);

            let raw = open_folder_input.get();
            let path = PathBuf::from(raw.trim());
            if path.is_dir() {
                add_workspace_root(state, path);
            }
        }
        AppAction::CancelOpenFolderDialog => {
            if let Some(modal) = state.lock().open_folder_modal.take() {
                desktop.wm.close(modal);
            }
        }

        AppAction::OpenPath { path, target } => {
            open_path(
                desktop,
                screen,
                state,
                actions.clone(),
                editor_theme,
                clipboard,
                target,
                path,
            );
        }
        AppAction::OpenPathAndJump { path, target } => {
            open_path_with_jump(
                desktop,
                screen,
                state,
                actions.clone(),
                editor_theme,
                clipboard,
                path,
                target,
            );
        }
        AppAction::JumpTo(target) => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::JumpTo(target));
            } else {
                set_status_message(state, "No active editor for jump target");
            }
        }
        AppAction::SelectEditorTab { window, tab_id } => {
            if let Some(cmds) = state.lock().editor_windows.get(&window).cloned()
                && desktop.wm.window(window).is_some()
            {
                desktop.focus_window(window);
                cmds.push(EditorWindowCommand::SelectTabById(tab_id));
            }
        }
        AppAction::ShowStatusMessage(message) => {
            set_status_message(state, message);
        }
    }

    Ok(AppControl::Continue)
}

pub(crate) fn handle_command_key_event(
    desktop: &mut Desktop,
    event: &Event,
    _screen: Rect,
    result: &DesktopEventResult,
    keymap: &mut KeySequenceEngine<AppCommandAction>,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) -> Result<AppControl> {
    if desktop.wm.has_active_modal() {
        keymap.clear_pending();
        desktop.clear_which_key();
        return Ok(AppControl::Continue);
    }

    let Event::Key(key) = event else {
        if !keymap.pending().is_empty() {
            keymap.clear_pending();
            desktop.clear_which_key();
        }
        return Ok(AppControl::Continue);
    };
    let Some(chord) = KeyChord::from_key_event(*key) else {
        return Ok(AppControl::Continue);
    };

    if key.code == KeyCode::Esc && !keymap.pending().is_empty() {
        keymap.clear_pending();
        desktop.clear_which_key();
        return Ok(AppControl::Continue);
    }

    if result.outcome != EventOutcome::Ignored {
        if !keymap.pending().is_empty() {
            keymap.clear_pending();
            desktop.clear_which_key();
        }
        return Ok(AppControl::Continue);
    }

    match keymap.handle_key(chord, Instant::now()) {
        KeymapMatch::None | KeymapMatch::Timeout => {
            desktop.clear_which_key();
        }
        KeymapMatch::Prefix { choices } => {
            desktop.set_which_key(Some(WhichKeyModel::for_prefix(keymap.pending(), choices)));
        }
        KeymapMatch::Exact(action) => {
            desktop.clear_which_key();
            execute_command_action(desktop, state, actions, action);
        }
        KeymapMatch::AmbiguousExact { action, choices } => {
            execute_command_action(desktop, state, actions, action);
            desktop.set_which_key(Some(WhichKeyModel::for_prefix(keymap.pending(), choices)));
        }
    }

    Ok(AppControl::Continue)
}

pub(crate) fn execute_command_action(
    desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
    action: AppCommandAction,
) {
    match action {
        AppCommandAction::App(action) => actions.push(action),
        AppCommandAction::EditorWindow(command) => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(command);
            }
        }
        AppCommandAction::Editor(action) => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::EditorAction(action));
            }
        }
        AppCommandAction::OpenCommandPalette => actions.push(AppAction::OpenCommandPalette),
    }
}
