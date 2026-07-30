//! Open-path helpers (open / open-with-jump / canonicalize) and workspace-root
//! management (add root, remove editor-window state).

use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn open_path(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: EventQueue<AppAction>,
    editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: atto_ui::reactive::Binding<String>,
    target: OpenTarget,
    path: PathBuf,
) {
    let path = canonicalize_best_effort(&path);

    // Treat directories as workspace roots.
    if path.is_dir() {
        add_workspace_root(state, path);
        return;
    }

    // Only add a new workspace root if this file is *outside* the current workspace roots.
    //
    // The Explorer already shows files that are under an existing root, and rebuilding the tree
    // can be expensive on large directories.
    let should_add_root = {
        let s = state.lock();
        !s.workspace_roots.iter().any(|root| path.starts_with(root))
    };
    if should_add_root {
        add_workspace_root(state, parent_dir_or_cwd(&path));
    }

    match target {
        OpenTarget::NewTab => {
            if let Some(cmds) = active_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::OpenFile(path));
            } else {
                add_editor_window(
                    desktop,
                    screen,
                    state,
                    actions,
                    editor_theme,
                    clipboard,
                    vec![path],
                );
            }
        }
        OpenTarget::NewWindow => {
            add_editor_window(
                desktop,
                screen,
                state,
                actions,
                editor_theme,
                clipboard,
                vec![path],
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn open_path_with_jump(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: EventQueue<AppAction>,
    editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: atto_ui::reactive::Binding<String>,
    path: PathBuf,
    target: JumpTarget,
) {
    let path = canonicalize_best_effort(&path);

    if path.is_dir() {
        add_workspace_root(state, path);
        set_status_message(state, "Opened folder; no jump target applied");
        return;
    }

    let should_add_root = {
        let s = state.lock();
        !s.workspace_roots.iter().any(|root| path.starts_with(root))
    };
    if should_add_root {
        add_workspace_root(state, parent_dir_or_cwd(&path));
    }

    if let Some(cmds) = active_editor_commands(desktop, state) {
        cmds.push(EditorWindowCommand::OpenFileAndJump { path, target });
    } else {
        let cmds = add_editor_window(
            desktop,
            screen,
            state,
            actions,
            editor_theme,
            clipboard,
            Vec::new(),
        );
        cmds.push(EditorWindowCommand::OpenFileAndJump { path, target });
    }
}

pub(crate) fn canonicalize_best_effort(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn add_workspace_root(state: &Arc<Mutex<AppState>>, root: PathBuf) {
    let root = canonicalize_best_effort(&root);

    let (roots, explorer_cmds) = {
        let mut s = state.lock();
        if s.workspace_roots.iter().any(|p| p == &root) {
            return;
        }
        s.workspace_roots.push(root);
        s.workspace_state
            .lock()
            .set_workspace_roots(s.workspace_roots.clone());
        s.file_picker_cache = None;
        (s.workspace_roots.clone(), s.explorer_commands.clone())
    };

    explorer_cmds.push(ExplorerWindowCommand::SetWorkspaceRoots(roots));
}

pub(crate) fn remove_editor_window_state(state: &Arc<Mutex<AppState>>, id: WindowId) {
    let mut s = state.lock();
    let unregister_result = s.workspace_state.lock().unregister_window(id);
    if let Err(err) = unregister_result {
        s.status_message = Some(err);
    }
    s.editor_windows.remove(&id);
    s.editor_events.remove(&id);
    s.editor_diagnostics.remove(&id);
    s.editor_statuses.remove(&id);
    s.editor_tab_summaries.remove(&id);
    if s.last_focused_editor == Some(id) {
        s.last_focused_editor = None;
    }
}
