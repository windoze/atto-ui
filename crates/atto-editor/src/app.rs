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
use atto_ui::wm::{Window, WindowId, WindowKind};

use crate::actions::{AppAction, OpenTarget};
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

#[derive(Default)]
struct AppState {
    editor_windows: HashMap<WindowId, EventQueue<EditorWindowCommand>>,
    next_window_offset: u16,
    open_folder_modal: Option<WindowId>,
    open_file_target: Option<OpenTarget>,
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

                // Initial editor window.
                let work = Desktop::layout(screen).work_area;
                let rect = default_editor_rect(work, 0);

                let (workspace_roots, initial_files) = split_initial_paths(&config.initial_paths);
                let commands = EventQueue::<EditorWindowCommand>::new();
                let view = EditorWindowView::new(
                    actions.clone(),
                    commands.clone(),
                    editor_theme.clone(),
                    clipboard.clone(),
                    workspace_roots,
                );

                let id = desktop.add_window(
                    Window::new(WindowKind::Normal, "Atto Editor", rect, Box::new(view))
                        .with_tag("atto-editor"),
                    screen,
                );
                state.lock().editor_windows.insert(id, commands.clone());

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
                    match target {
                        OpenTarget::NewTab => {
                            if let Some(cmds) = focused_editor_commands(desktop, &state) {
                                cmds.push(EditorWindowCommand::OpenFile(path));
                            } else {
                                add_editor_window(
                                    desktop,
                                    screen,
                                    &state,
                                    actions.clone(),
                                    editor_theme.clone(),
                                    clipboard.clone(),
                                    vec![parent_dir_or_cwd(&path)],
                                    vec![path],
                                );
                            }
                        }
                        OpenTarget::NewWindow => {
                            add_editor_window(
                                desktop,
                                screen,
                                &state,
                                actions.clone(),
                                editor_theme.clone(),
                                clipboard.clone(),
                                vec![parent_dir_or_cwd(&path)],
                                vec![path],
                            );
                        }
                    }
                }

                if let Some(path) = save_as_result.get() {
                    save_as_result.set(None);
                    if let Some(cmds) = focused_editor_commands(desktop, &state) {
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
                MenuItem::action("Toggle Explorer", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::ToggleExplorer)
                }),
                MenuItem::action("Explorer Left", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::ExplorerLeft)
                }),
                MenuItem::action("Explorer Right", {
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
            if let Some(cmds) = focused_editor_commands(desktop, state) {
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
            if let Some(cmds) = focused_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::CloseActiveTab);
            }
        }
        AppAction::SplitVertical => {
            if let Some(cmds) = focused_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::SplitVertical);
            }
        }
        AppAction::SplitHorizontal => {
            if let Some(cmds) = focused_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::SplitHorizontal);
            }
        }
        AppAction::CloseSplit => {
            if let Some(cmds) = focused_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::CloseSplit);
            }
        }

        AppAction::ToggleExplorer => {
            if let Some(cmds) = focused_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::ToggleSidebar);
            }
        }
        AppAction::ExplorerLeft => {
            if let Some(cmds) = focused_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::SidebarLeft);
            }
        }
        AppAction::ExplorerRight => {
            if let Some(cmds) = focused_editor_commands(desktop, state) {
                cmds.push(EditorWindowCommand::SidebarRight);
            }
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
            if path.is_dir()
                && let Some(cmds) = focused_editor_commands(desktop, state)
            {
                cmds.push(EditorWindowCommand::AddWorkspaceRoot(path));
            }
        }
        AppAction::CancelOpenFolderDialog => {
            if let Some(modal) = state.lock().open_folder_modal.take() {
                desktop.wm.close(modal);
            }
        }

        AppAction::OpenFileInNewWindow(path) => {
            add_editor_window(
                desktop,
                screen,
                state,
                actions.clone(),
                editor_theme,
                clipboard,
                vec![parent_dir_or_cwd(&path)],
                vec![path],
            );
        }
    }

    Ok(AppControl::Continue)
}

fn focused_editor_commands(
    desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
) -> Option<EventQueue<EditorWindowCommand>> {
    let focused = desktop.wm.focused()?;
    state.lock().editor_windows.get(&focused).cloned()
}

fn add_editor_window(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: EventQueue<AppAction>,
    editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: atto_ui::reactive::Binding<String>,
    workspace_roots: Vec<PathBuf>,
    initial_files: Vec<PathBuf>,
) {
    let work = Desktop::layout(screen).work_area;
    let offset = state.lock().next_window_offset;
    state.lock().next_window_offset = offset.saturating_add(2);

    let rect = default_editor_rect(work, offset);
    let commands = EventQueue::<EditorWindowCommand>::new();
    let view = EditorWindowView::new(
        actions,
        commands.clone(),
        editor_theme,
        clipboard,
        workspace_roots,
    );
    let id = desktop.add_window(
        Window::new(WindowKind::Normal, "Atto Editor", rect, Box::new(view))
            .with_tag("atto-editor"),
        screen,
    );
    state.lock().editor_windows.insert(id, commands.clone());

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
