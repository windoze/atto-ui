use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use parking_lot::Mutex;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    run_crossterm_desktop,
};
use atto_ui::composable::{Component, HStack, LayoutParams, Size, TextFn, VStack};
use atto_ui::dialogs::FileDialog;
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::{Button, TextBox};
use atto_ui::wm::{DockAutoHide, DockSide, Window, WindowDock, WindowId, WindowKind};

use crate::actions::{AppAction, OpenTarget};
use crate::explorer_window::{ExplorerWindowCommand, ExplorerWindowView};
use crate::window::{EditorWindowCommand, EditorWindowView};

#[derive(Clone, Debug)]
pub struct AttoEditorConfig {
    pub initial_paths: Vec<PathBuf>,
}

impl AttoEditorConfig {
    pub fn from_env_args() -> Self {
        let initial_paths = env::args().skip(1).map(PathBuf::from).collect();
        Self { initial_paths }
    }
}

const DEFAULT_EXPLORER_DOCK_SIZE: u16 = 34;
const MIN_EXPLORER_DOCK_SIZE: u16 = 20;

struct AppState {
    editor_windows: HashMap<WindowId, EventQueue<EditorWindowCommand>>,
    last_focused_editor: Option<WindowId>,
    next_window_offset: u16,

    explorer_window: Option<WindowId>,
    explorer_commands: EventQueue<ExplorerWindowCommand>,
    explorer_dock: DockSide,
    explorer_size: Option<u16>,
    workspace_roots: Vec<PathBuf>,

    open_folder_modal: Option<WindowId>,
    open_file_target: Option<OpenTarget>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            editor_windows: HashMap::new(),
            last_focused_editor: None,
            next_window_offset: 0,
            explorer_window: None,
            explorer_commands: EventQueue::new(),
            explorer_dock: DockSide::Left,
            explorer_size: None,
            workspace_roots: Vec::new(),
            open_folder_modal: None,
            open_file_target: None,
        }
    }
}

pub fn run(config: AttoEditorConfig) -> Result<()> {
    let actions: EventQueue<AppAction> = EventQueue::new();

    let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
        atto_ui_editor::EditorThemeSet::default().into();
    let clipboard: atto_ui::reactive::Binding<String> = String::new().into();

    let open_file_result: Property<Option<PathBuf>> = Property::new(None);
    let save_as_result: Property<Option<PathBuf>> = Property::new(None);
    let open_folder_input: Property<String> = Property::new(String::new());

    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));

    let app_cfg = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .bracketed_paste(true)
        .cursor(CursorMode::Show)
        .mouse_capture(true);

    run_crossterm_desktop(
        app_cfg,
        {
            let actions = actions.clone();
            let state = state.clone();
            let open_file_result = open_file_result.clone();
            let save_as_result = save_as_result.clone();
            let open_folder_input = open_folder_input.clone();
            let editor_theme = editor_theme.clone();
            let clipboard = clipboard.clone();

            move |screen: Rect| {
                let menu = build_menu(actions.clone());
                let mut desktop = Desktop::new(Theme::dark(), menu);

                let (workspace_roots, initial_files) = split_initial_paths(&config.initial_paths);
                let workspace_roots = workspace_roots
                    .into_iter()
                    .map(|p| canonicalize_best_effort(&p))
                    .collect::<Vec<_>>();

                {
                    let mut s = state.lock();
                    s.workspace_roots = workspace_roots.clone();
                }

                // Explorer window (file tree).
                let (explorer_side, explorer_size, explorer_commands) = {
                    let s = state.lock();
                    (
                        s.explorer_dock,
                        s.explorer_size.unwrap_or(DEFAULT_EXPLORER_DOCK_SIZE),
                        s.explorer_commands.clone(),
                    )
                };
                let explorer_view = ExplorerWindowView::new(
                    actions.clone(),
                    explorer_commands.clone(),
                    workspace_roots.clone(),
                );
                let explorer_id = desktop.add_window(
                    Window::new(
                        WindowKind::Normal,
                        "Explorer",
                        Rect::default(),
                        Box::new(explorer_view),
                    )
                    .with_dock(Some(explorer_dock_config(explorer_side, explorer_size)))
                    .with_tag("atto-editor-app-explorer")
                    .with_close_hook({
                        let state = state.clone();
                        move |id| {
                            let mut s = state.lock();
                            if s.explorer_window == Some(id) {
                                s.explorer_window = None;
                            }
                            true
                        }
                    }),
                    screen,
                );
                {
                    let mut s = state.lock();
                    s.explorer_window = Some(explorer_id);
                    s.explorer_dock = explorer_side;
                    s.explorer_size = Some(explorer_size);
                }

                // Initial editor window.
                let work = Desktop::layout(screen).work_area;
                let rect = default_editor_rect(work, 0);
                let commands = EventQueue::<EditorWindowCommand>::new();
                let view = EditorWindowView::new(
                    actions.clone(),
                    commands.clone(),
                    editor_theme.clone(),
                    clipboard.clone(),
                );

                let id = desktop.add_window(
                    Window::new(WindowKind::Normal, "Atto Editor", rect, Box::new(view))
                        .with_tag("atto-editor-app"),
                    screen,
                );
                {
                    let mut s = state.lock();
                    s.editor_windows.insert(id, commands.clone());
                    s.last_focused_editor = Some(id);
                }

                // Seed initial files (best-effort).
                for file in initial_files {
                    commands.push(EditorWindowCommand::OpenFile(file));
                }

                // Quiet unused captures (these are used in on_tick via clones).
                let _ = (open_file_result, save_as_result, open_folder_input);

                Ok(desktop)
            }
        },
        {
            let actions = actions.clone();
            let state = state.clone();
            let open_file_result = open_file_result.clone();
            let save_as_result = save_as_result.clone();
            let open_folder_input = open_folder_input.clone();
            let editor_theme = editor_theme.clone();
            let clipboard = clipboard.clone();

            move |desktop: &mut Desktop, screen: Rect| {
                // Track the last focused editor window so actions from non-editor windows (like the
                // Explorer) still know where to open files.
                if let Some(focused) = desktop.wm.focused() {
                    let mut s = state.lock();
                    if s.editor_windows.contains_key(&focused) {
                        s.last_focused_editor = Some(focused);
                    }
                }

                sync_explorer_dock_state(desktop, &state);

                // Handle queued UI actions (menus / dialogs / child windows).
                for action in actions.drain() {
                    if handle_action(
                        desktop,
                        screen,
                        &state,
                        &actions,
                        action,
                        &open_file_result,
                        &save_as_result,
                        &open_folder_input,
                        editor_theme.clone(),
                        clipboard.clone(),
                    )? == AppControl::Exit
                    {
                        return Ok(AppControl::Exit);
                    }
                }

                // Handle dialog results (FileDialog writes into bindings and closes itself).
                if let Some(path) = open_file_result.get() {
                    open_file_result.set(None);
                    let target = state
                        .lock()
                        .open_file_target
                        .take()
                        .unwrap_or(OpenTarget::NewTab);
                    open_path(
                        desktop,
                        screen,
                        &state,
                        actions.clone(),
                        editor_theme.clone(),
                        clipboard.clone(),
                        target,
                        path,
                    );
                }

                if let Some(path) = save_as_result.get() {
                    save_as_result.set(None);
                    if let Some(cmds) = active_editor_commands(desktop, &state) {
                        cmds.push(EditorWindowCommand::SaveAs(path));
                    }
                }

                Ok(AppControl::Continue)
            }
        },
        |_desktop, _event, _screen, _res| Ok(AppControl::Continue),
    )
}

fn build_menu(actions: EventQueue<AppAction>) -> MenuBar {
    MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::action("Open File…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenFileDialog(OpenTarget::NewTab))
                })
                .shortcut("Ctrl+O"),
                MenuItem::action("Open File (New Window)…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenFileDialog(OpenTarget::NewWindow))
                }),
                MenuItem::action("Open Folder…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::OpenFolderDialog)
                }),
                MenuItem::action("Save", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Save)
                })
                .shortcut("Ctrl+S"),
                MenuItem::action("Save As…", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::SaveAsDialog)
                }),
                MenuItem::action("Quit", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Quit)
                })
                .shortcut("Ctrl+Q"),
            ],
        ),
        MenuSpec::new(
            "View",
            vec![
                MenuItem::action("Toggle Explorer Window", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::ToggleExplorer)
                })
                .shortcut("Ctrl+E"),
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
            "Split",
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

#[allow(clippy::too_many_arguments)]
fn handle_action(
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
    }

    Ok(AppControl::Continue)
}

#[allow(clippy::too_many_arguments)]
fn open_path(
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

fn canonicalize_best_effort(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn add_workspace_root(state: &Arc<Mutex<AppState>>, root: PathBuf) {
    let root = canonicalize_best_effort(&root);

    let (roots, explorer_cmds) = {
        let mut s = state.lock();
        if s.workspace_roots.iter().any(|p| p == &root) {
            return;
        }
        s.workspace_roots.push(root);
        (s.workspace_roots.clone(), s.explorer_commands.clone())
    };

    explorer_cmds.push(ExplorerWindowCommand::SetWorkspaceRoots(roots));
}

fn sync_explorer_dock_state(desktop: &Desktop, state: &Arc<Mutex<AppState>>) {
    let Some(id) = state.lock().explorer_window else {
        return;
    };

    let Some(window) = desktop.wm.window(id) else {
        state.lock().explorer_window = None;
        return;
    };

    if let Some(dock) = window.dock.get() {
        let mut s = state.lock();
        s.explorer_dock = dock.side;
        s.explorer_size = Some(dock.size);
    }
}

fn explorer_dock_config(side: DockSide, size: u16) -> WindowDock {
    WindowDock {
        side,
        size,
        min_size: MIN_EXPLORER_DOCK_SIZE,
        max_size: None,
        auto_hide: DockAutoHide::Disabled,
        handle_label: Some("Explorer".to_string()),
    }
}

fn toggle_explorer_window(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: EventQueue<AppAction>,
) {
    // Close if currently open.
    let open_id = state.lock().explorer_window;
    if let Some(id) = open_id {
        if let Some(w) = desktop.wm.window(id) {
            if let Some(dock) = w.dock.get() {
                let mut s = state.lock();
                s.explorer_dock = dock.side;
                s.explorer_size = Some(dock.size);
            }
            desktop.wm.close(id);
            state.lock().explorer_window = None;
            return;
        }

        // Window is gone (closed elsewhere).
        state.lock().explorer_window = None;
    }

    // Open a new Explorer window.
    let (side, size, roots, explorer_cmds) = {
        let s = state.lock();
        (
            s.explorer_dock,
            s.explorer_size.unwrap_or(DEFAULT_EXPLORER_DOCK_SIZE),
            s.workspace_roots.clone(),
            s.explorer_commands.clone(),
        )
    };

    let view = ExplorerWindowView::new(actions.clone(), explorer_cmds, roots);
    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Explorer",
            Rect::default(),
            Box::new(view),
        )
        .with_dock(Some(explorer_dock_config(side, size)))
        .with_tag("atto-editor-app-explorer")
        .with_close_hook({
            let state = state.clone();
            move |id| {
                let mut s = state.lock();
                if s.explorer_window == Some(id) {
                    s.explorer_window = None;
                }
                true
            }
        }),
        screen,
    );

    let mut s = state.lock();
    s.explorer_window = Some(id);
    s.explorer_dock = side;
    s.explorer_size = Some(size);
}

fn dock_explorer_window(
    desktop: &mut Desktop,
    _screen: Rect,
    state: &Arc<Mutex<AppState>>,
    side: DockSide,
) {
    let (size, id) = {
        let s = state.lock();
        (
            s.explorer_size.unwrap_or(DEFAULT_EXPLORER_DOCK_SIZE),
            s.explorer_window,
        )
    };

    {
        let mut s = state.lock();
        s.explorer_dock = side;
        s.explorer_size = Some(size);
    }

    if let Some(id) = id {
        if let Some(w) = desktop.wm.window_mut(id) {
            let mut dock = w
                .dock
                .get()
                .unwrap_or_else(|| explorer_dock_config(side, size));
            dock.side = side;
            dock.size = size;
            dock.min_size = MIN_EXPLORER_DOCK_SIZE;
            dock.auto_hide = DockAutoHide::Disabled;
            if dock.handle_label.is_none() {
                dock.handle_label = Some("Explorer".to_string());
            }
            w.dock.set(Some(dock));
        } else {
            state.lock().explorer_window = None;
        }
    }
}

fn active_editor_commands(
    desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
) -> Option<EventQueue<EditorWindowCommand>> {
    let focused = desktop.wm.focused();

    // We must be careful not to hold the `state` lock across an `if let` body and then try to
    // lock it again (that would deadlock). Grab the needed values in a single scoped lock.
    {
        let guard = state.lock();

        // Prefer the currently focused window if it's an editor.
        if let Some(id) = focused
            && let Some(cmds) = guard.editor_windows.get(&id)
        {
            return Some(cmds.clone());
        }

        // Fall back to the last-focused editor window (useful when Explorer is focused).
        if let Some(id) = guard.last_focused_editor
            && desktop.wm.window(id).is_some()
            && let Some(cmds) = guard.editor_windows.get(&id)
        {
            return Some(cmds.clone());
        }
    }

    // Finally, fall back to the topmost editor window in z-order.
    let guard = state.lock();
    for w in desktop.wm.windows().iter().rev() {
        if let Some(cmds) = guard.editor_windows.get(&w.id()) {
            return Some(cmds.clone());
        }
    }

    None
}

fn add_editor_window(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: EventQueue<AppAction>,
    editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: atto_ui::reactive::Binding<String>,
    initial_files: Vec<PathBuf>,
) {
    let work = Desktop::layout(screen).work_area;
    let offset = state.lock().next_window_offset;
    state.lock().next_window_offset = offset.saturating_add(2);

    let rect = default_editor_rect(work, offset);
    let commands = EventQueue::<EditorWindowCommand>::new();
    let view = EditorWindowView::new(actions, commands.clone(), editor_theme, clipboard);
    let id = desktop.add_window(
        Window::new(WindowKind::Normal, "Atto Editor", rect, Box::new(view))
            .with_tag("atto-editor-app"),
        screen,
    );
    {
        let mut s = state.lock();
        s.editor_windows.insert(id, commands.clone());
        s.last_focused_editor = Some(id);
    }

    for file in initial_files {
        commands.push(EditorWindowCommand::OpenFile(file));
    }
}

fn build_open_folder_view(
    input: Property<String>,
    actions: EventQueue<AppAction>,
) -> Box<dyn Component> {
    let input_binding = input.binding();

    let open_button = Button::new("Open").on_click({
        let actions = actions.clone();
        move || actions.push(AppAction::SubmitOpenFolderDialog)
    });
    let cancel_button =
        Button::new("Cancel").on_click(move || actions.push(AppAction::CancelOpenFolderDialog));

    let help = TextFn::new(|| "Tip: paste a folder path and press Open.".to_string());

    let layout = VStack::new()
        .spacing(1)
        .padding(1)
        .child_with_layout(
            TextBox::new("Folder", input_binding),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            help,
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            HStack::new()
                .spacing(1)
                .child(open_button)
                .child(cancel_button),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        );

    Box::new(layout)
}

fn centered_rect(work: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(work.width.saturating_sub(2)).max(20);
    let h = height.min(work.height.saturating_sub(2)).max(8);
    Rect {
        x: work.x + (work.width.saturating_sub(w)) / 2,
        y: work.y + (work.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn default_editor_rect(work: Rect, offset: u16) -> Rect {
    let w = work.width.saturating_sub(6).max(40);
    let h = work.height.saturating_sub(4).max(12);
    Rect {
        x: work
            .x
            .saturating_add(3)
            .saturating_add(offset)
            .min(work.x + work.width.saturating_sub(1)),
        y: work
            .y
            .saturating_add(2)
            .saturating_add(offset)
            .min(work.y + work.height.saturating_sub(1)),
        width: w.min(work.width.saturating_sub(2)),
        height: h.min(work.height.saturating_sub(2)),
    }
}

fn split_initial_paths(paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut roots = Vec::<PathBuf>::new();
    let mut files = Vec::<PathBuf>::new();

    if paths.is_empty() {
        roots.push(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        return (roots, files);
    }

    for p in paths {
        if p.is_dir() {
            roots.push(p.clone());
        } else if p.is_file() {
            files.push(p.clone());
        }
    }

    if roots.is_empty() {
        if let Some(parent) = files
            .first()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
        {
            roots.push(parent);
        } else {
            roots.push(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        }
    }

    (roots, files)
}

fn parent_dir_or_cwd(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("atto_editor_app_{prefix}_{nanos}"))
    }

    #[test]
    fn opening_file_inside_workspace_does_not_add_new_root() {
        let root = unique_temp_dir("workspace_open");
        fs::create_dir_all(root.join("src")).expect("create temp dirs");
        let file = root.join("src").join("main.rs");
        fs::write(&file, "fn main() {}\n").expect("write temp file");

        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        {
            let mut s = state.lock();
            s.workspace_roots = vec![canonicalize_best_effort(&root)];
        }

        let actions: EventQueue<AppAction> = EventQueue::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();

        let commands = EventQueue::<EditorWindowCommand>::new();
        let view = EditorWindowView::new(
            actions.clone(),
            commands.clone(),
            editor_theme.clone(),
            clipboard.clone(),
        );
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                Rect::new(0, 0, 60, 16),
                Box::new(view),
            )
            .with_tag("atto-editor-app"),
            screen,
        );
        {
            let mut s = state.lock();
            s.editor_windows.insert(editor_id, commands);
            s.last_focused_editor = Some(editor_id);
        }

        open_path(
            &mut desktop,
            screen,
            &state,
            actions,
            editor_theme,
            clipboard,
            OpenTarget::NewTab,
            file,
        );

        let s = state.lock();
        assert_eq!(s.workspace_roots.len(), 1);
        assert!(s.explorer_commands.is_empty());
    }

    #[test]
    fn explorer_window_uses_wm_dock_and_editor_clamps_to_reserved_area() {
        let screen = Rect::new(0, 0, 90, 28);
        let work = Desktop::layout(screen).work_area;
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();

        let explorer = ExplorerWindowView::new(
            actions.clone(),
            state.lock().explorer_commands.clone(),
            Vec::new(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );
        state.lock().explorer_window = Some(explorer_id);

        let editor_commands = EventQueue::<EditorWindowCommand>::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();
        let editor = EditorWindowView::new(actions, editor_commands, editor_theme, clipboard);
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                default_editor_rect(work, 0),
                Box::new(editor),
            ),
            screen,
        );

        assert_eq!(
            desktop.wm.window(explorer_id).expect("explorer").rect.get(),
            Rect::new(0, 1, DEFAULT_EXPLORER_DOCK_SIZE, 26)
        );

        let editor_rect = desktop.wm.window(editor_id).expect("editor").rect.get();
        assert!(
            editor_rect.x >= DEFAULT_EXPLORER_DOCK_SIZE,
            "editor should be clamped into WM effective work area, got {editor_rect:?}"
        );
    }

    #[test]
    fn dock_explorer_window_updates_dock_side_and_preserves_size() {
        let screen = Rect::new(0, 0, 90, 28);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let explorer = ExplorerWindowView::new(
            actions.clone(),
            state.lock().explorer_commands.clone(),
            Vec::new(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(DockSide::Left, 41))),
            screen,
        );
        {
            let mut s = state.lock();
            s.explorer_window = Some(explorer_id);
            s.explorer_size = Some(41);
        }

        dock_explorer_window(&mut desktop, screen, &state, DockSide::Right);

        let dock = desktop
            .wm
            .window(explorer_id)
            .expect("explorer")
            .dock
            .get()
            .expect("dock config");
        assert_eq!(dock.side, DockSide::Right);
        assert_eq!(dock.size, 41);
        assert_eq!(state.lock().explorer_dock, DockSide::Right);
        assert_eq!(state.lock().explorer_size, Some(41));

        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| desktop.draw(f)).expect("draw");

        assert_eq!(
            desktop.wm.window(explorer_id).expect("explorer").rect.get(),
            Rect::new(49, 1, 41, 26)
        );
    }

    #[test]
    fn sync_explorer_dock_state_records_resized_dock_size() {
        let screen = Rect::new(0, 0, 90, 28);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let explorer =
            ExplorerWindowView::new(actions, state.lock().explorer_commands.clone(), Vec::new());
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );
        state.lock().explorer_window = Some(explorer_id);

        let mut dock = desktop
            .wm
            .window(explorer_id)
            .expect("explorer")
            .dock
            .get()
            .expect("dock config");
        dock.size = 43;
        dock.side = DockSide::Right;
        desktop
            .wm
            .window_mut(explorer_id)
            .expect("explorer")
            .dock
            .set(Some(dock));

        sync_explorer_dock_state(&desktop, &state);

        let s = state.lock();
        assert_eq!(s.explorer_dock, DockSide::Right);
        assert_eq!(s.explorer_size, Some(43));
    }

    #[test]
    fn toggle_explorer_window_preserves_dock_side_and_size_on_reopen() {
        let screen = Rect::new(0, 0, 90, 28);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        {
            let mut s = state.lock();
            s.explorer_dock = DockSide::Right;
            s.explorer_size = Some(41);
        }
        toggle_explorer_window(&mut desktop, screen, &state, actions.clone());
        let first_id = state.lock().explorer_window.expect("explorer reopened");

        let dock = desktop
            .wm
            .window(first_id)
            .expect("explorer")
            .dock
            .get()
            .expect("dock config");
        assert_eq!(dock.side, DockSide::Right);
        assert_eq!(dock.size, 41);

        toggle_explorer_window(&mut desktop, screen, &state, actions.clone());
        assert!(state.lock().explorer_window.is_none());

        toggle_explorer_window(&mut desktop, screen, &state, actions);
        let reopened_id = state.lock().explorer_window.expect("explorer reopened");
        assert_ne!(first_id, reopened_id);
        let reopened_dock = desktop
            .wm
            .window(reopened_id)
            .expect("explorer")
            .dock
            .get()
            .expect("dock config");
        assert_eq!(reopened_dock.side, DockSide::Right);
        assert_eq!(reopened_dock.size, 41);
    }

    #[test]
    fn active_editor_commands_falls_back_when_explorer_is_focused() {
        let screen = Rect::new(0, 0, 90, 28);
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let actions: EventQueue<AppAction> = EventQueue::new();
        let menu = MenuBar::new(Vec::new());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let explorer = ExplorerWindowView::new(
            actions.clone(),
            state.lock().explorer_commands.clone(),
            Vec::new(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );

        let editor_commands = EventQueue::<EditorWindowCommand>::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();
        let editor =
            EditorWindowView::new(actions, editor_commands.clone(), editor_theme, clipboard);
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                default_editor_rect(Desktop::layout(screen).work_area, 0),
                Box::new(editor),
            ),
            screen,
        );
        {
            let mut s = state.lock();
            s.explorer_window = Some(explorer_id);
            s.editor_windows.insert(editor_id, editor_commands.clone());
            s.last_focused_editor = Some(editor_id);
        }

        desktop.wm.focus(explorer_id);
        assert_eq!(desktop.wm.focused(), Some(explorer_id));

        active_editor_commands(&desktop, &state)
            .expect("last focused editor commands")
            .push(EditorWindowCommand::SaveActive);

        let drained = editor_commands.drain();
        assert!(matches!(
            drained.as_slice(),
            [EditorWindowCommand::SaveActive]
        ));
    }

    #[test]
    fn explorer_double_click_opens_file_in_editor() -> anyhow::Result<()> {
        let root = unique_temp_dir("explorer_double_click_opens");
        fs::create_dir_all(&root)?;
        let file_path = root.join("open_me.txt");
        fs::write(&file_path, "HELLO_FROM_EDITOR\n")?;

        let screen = Rect::new(0, 0, 90, 28);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();

        let open_file_result: Property<Option<PathBuf>> = Property::new(None);
        let save_as_result: Property<Option<PathBuf>> = Property::new(None);
        let open_folder_input: Property<String> = Property::new(String::new());

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        state.lock().workspace_roots = vec![canonicalize_best_effort(&root)];

        let menu = build_menu(actions.clone());
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let work = Desktop::layout(screen).work_area;

        // Explorer window.
        let explorer_commands = state.lock().explorer_commands.clone();
        let explorer_view = ExplorerWindowView::new(
            actions.clone(),
            explorer_commands.clone(),
            state.lock().workspace_roots.clone(),
        );
        let explorer_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Explorer",
                Rect::default(),
                Box::new(explorer_view),
            )
            .with_dock(Some(explorer_dock_config(
                DockSide::Left,
                DEFAULT_EXPLORER_DOCK_SIZE,
            ))),
            screen,
        );
        state.lock().explorer_window = Some(explorer_id);

        // Editor window.
        let editor_rect = default_editor_rect(work, 0);
        let editor_commands: EventQueue<EditorWindowCommand> = EventQueue::new();
        let editor_view = EditorWindowView::new(
            actions.clone(),
            editor_commands.clone(),
            editor_theme.clone(),
            clipboard.clone(),
        );
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                editor_rect,
                Box::new(editor_view),
            ),
            screen,
        );
        {
            let mut s = state.lock();
            s.editor_windows.insert(editor_id, editor_commands.clone());
            s.last_focused_editor = Some(editor_id);
        }

        // Draw once so scrollable children have a `last_area` for mouse routing.
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| desktop.draw(f))?;

        let inner = desktop
            .wm
            .window(explorer_id)
            .expect("explorer window")
            .inner_rect();
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: inner.x + 4,
            row: inner.y + 2,
            modifiers: KeyModifiers::NONE,
        });

        // First click selects; second click should trigger open action.
        let _ = desktop.handle_event(&click, screen);
        let _ = desktop.handle_event(&click, screen);

        // Process queued actions (like the main tick loop does).
        for action in actions.drain() {
            handle_action(
                &mut desktop,
                screen,
                &state,
                &actions,
                action,
                &open_file_result,
                &save_as_result,
                &open_folder_input,
                editor_theme.clone(),
                clipboard.clone(),
            )?;
        }

        terminal.draw(|f| desktop.draw(f))?;

        let mut snapshot = String::new();
        let buf = terminal.backend().buffer();
        for y in 0..screen.height {
            for x in 0..screen.width {
                snapshot.push_str(buf[(x, y)].symbol());
            }
            snapshot.push('\n');
        }

        assert!(
            snapshot.contains("HELLO_FROM_EDITOR"),
            "expected editor to render file contents; got:\n{snapshot}"
        );

        Ok(())
    }
}
