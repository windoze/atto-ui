use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use parking_lot::Mutex;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, DEFAULT_KEY_SEQUENCE_TIMEOUT, Desktop,
    DesktopEventResult, DesktopMode, KeyChord, KeySequenceEngine, KeymapMatch, MenuBar, MenuItem,
    MenuSpec, StatusSegment, StatusSegmentAlign, WhichKeyModel, run_crossterm_desktop,
};
use atto_ui::composable::{Component, EventOutcome, HStack, LayoutParams, Size, TextFn, VStack};
use atto_ui::dialogs::FileDialog;
use atto_ui::reactive::{Binding, EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::{Button, TextBox};
use atto_ui::wm::{DockAutoHide, DockSide, Window, WindowDock, WindowId, WindowKind};
use editor_core::{DocumentOutline, DocumentSymbol, SearchOptions, SymbolKind, WorkspaceSymbol};

use crate::actions::{AppAction, JumpTarget, OpenTarget};
use crate::commands::{self, AppCommandAction};
use crate::explorer_window::{ExplorerWindowCommand, ExplorerWindowView};
use crate::lsp_workspace::LspWorkspaceEvent;
use crate::picker::{PickerEvent, PickerItem, PickerView};
use crate::search::{GlobalSearchConfig, GlobalSearchResult, search_workspace};
use crate::window::{
    EditorStatus, EditorTabSummary, EditorWindowBindings, EditorWindowCommand, EditorWindowView,
};
use crate::workspace::{WorkspaceFileIndex, build_workspace_file_index};
use crate::workspace_state::{SharedWorkspaceState, WorkspaceState};

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
const MAX_FILE_PICKER_ENTRIES: usize = 20_000;
const MAX_SYMBOL_PICKER_RESULTS: usize = 300;
const MAX_GLOBAL_SEARCH_RESULTS: usize = 1_000;

struct AppState {
    editor_windows: HashMap<WindowId, EventQueue<EditorWindowCommand>>,
    editor_events: HashMap<WindowId, EventQueue<atto_ui_editor::EditorEvent>>,
    editor_diagnostics: HashMap<WindowId, Binding<atto_ui_editor::DiagnosticsSummary>>,
    editor_statuses: HashMap<WindowId, Binding<EditorStatus>>,
    editor_tab_summaries: HashMap<WindowId, Binding<Vec<EditorTabSummary>>>,
    last_focused_editor: Option<WindowId>,
    workspace_state: SharedWorkspaceState,
    status_message: Option<String>,
    next_window_offset: u16,

    explorer_window: Option<WindowId>,
    explorer_commands: EventQueue<ExplorerWindowCommand>,
    explorer_dock: DockSide,
    explorer_size: Option<u16>,
    workspace_roots: Vec<PathBuf>,

    open_folder_modal: Option<WindowId>,
    open_file_target: Option<OpenTarget>,
    command_palette_window: Option<WindowId>,
    command_palette_restore_focus: Option<WindowId>,
    command_palette_events: EventQueue<PickerEvent<AppCommandAction>>,
    file_picker_window: Option<WindowId>,
    file_picker_restore_focus: Option<WindowId>,
    file_picker_events: EventQueue<PickerEvent<AppAction>>,
    file_picker_cache: Option<WorkspaceFileIndex>,
    buffer_picker_window: Option<WindowId>,
    buffer_picker_restore_focus: Option<WindowId>,
    buffer_picker_events: EventQueue<PickerEvent<AppAction>>,
    document_symbol_picker_window: Option<WindowId>,
    document_symbol_picker_restore_focus: Option<WindowId>,
    document_symbol_picker_events: EventQueue<PickerEvent<AppAction>>,
    workspace_symbol_picker_window: Option<WindowId>,
    workspace_symbol_picker_restore_focus: Option<WindowId>,
    workspace_symbol_picker_events: EventQueue<PickerEvent<AppAction>>,
    global_search_picker_window: Option<WindowId>,
    global_search_picker_restore_focus: Option<WindowId>,
    global_search_picker_events: EventQueue<PickerEvent<AppAction>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            editor_windows: HashMap::new(),
            editor_events: HashMap::new(),
            editor_diagnostics: HashMap::new(),
            editor_statuses: HashMap::new(),
            editor_tab_summaries: HashMap::new(),
            last_focused_editor: None,
            workspace_state: WorkspaceState::shared(),
            status_message: None,
            next_window_offset: 0,
            explorer_window: None,
            explorer_commands: EventQueue::new(),
            explorer_dock: DockSide::Left,
            explorer_size: None,
            workspace_roots: Vec::new(),
            open_folder_modal: None,
            open_file_target: None,
            command_palette_window: None,
            command_palette_restore_focus: None,
            command_palette_events: EventQueue::new(),
            file_picker_window: None,
            file_picker_restore_focus: None,
            file_picker_events: EventQueue::new(),
            file_picker_cache: None,
            buffer_picker_window: None,
            buffer_picker_restore_focus: None,
            buffer_picker_events: EventQueue::new(),
            document_symbol_picker_window: None,
            document_symbol_picker_restore_focus: None,
            document_symbol_picker_events: EventQueue::new(),
            workspace_symbol_picker_window: None,
            workspace_symbol_picker_restore_focus: None,
            workspace_symbol_picker_events: EventQueue::new(),
            global_search_picker_window: None,
            global_search_picker_restore_focus: None,
            global_search_picker_events: EventQueue::new(),
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
    let mut command_keymap =
        commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);

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
                    if s.workspace_roots != workspace_roots {
                        s.workspace_roots = workspace_roots.clone();
                        s.workspace_state
                            .lock()
                            .set_workspace_roots(workspace_roots.clone());
                        s.file_picker_cache = None;
                    }
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
                let editor_events = EventQueue::<atto_ui_editor::EditorEvent>::new();
                let diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary> =
                    atto_ui_editor::DiagnosticsSummary::default().into();
                let editor_status: Binding<EditorStatus> = EditorStatus::default().into();
                let tab_summaries: Binding<Vec<EditorTabSummary>> = Vec::new().into();
                let workspace_state = state.lock().workspace_state.clone();
                let view = EditorWindowView::new_with_workspace_bindings(
                    actions.clone(),
                    commands.clone(),
                    editor_events.clone(),
                    editor_theme.clone(),
                    clipboard.clone(),
                    workspace_state,
                    EditorWindowBindings::new(
                        diagnostics_summary.clone(),
                        editor_status.clone(),
                        tab_summaries.clone(),
                    ),
                );

                let id = desktop.add_window(
                    Window::new(WindowKind::Normal, "Atto Editor", rect, Box::new(view))
                        .with_tag("atto-editor-app")
                        .with_close_hook({
                            let state = state.clone();
                            move |id| {
                                remove_editor_window_state(&state, id);
                                true
                            }
                        }),
                    screen,
                );
                {
                    let mut s = state.lock();
                    s.editor_windows.insert(id, commands.clone());
                    s.editor_events.insert(id, editor_events);
                    s.editor_diagnostics.insert(id, diagnostics_summary);
                    s.editor_statuses.insert(id, editor_status);
                    s.editor_tab_summaries.insert(id, tab_summaries);
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
                update_diagnostics_statusbar(desktop, &state);

                process_command_palette_events(desktop, &state, &actions);
                process_file_picker_events(desktop, &state, &actions);
                process_buffer_picker_events(desktop, &state, &actions);
                process_document_symbol_picker_events(desktop, &state, &actions);
                process_workspace_symbol_picker_events(desktop, screen, &state, &actions);
                process_global_search_picker_events(desktop, screen, &state, &actions);
                process_workspace_lsp_events(desktop, screen, &state);
                process_editor_events(desktop, screen, &state);

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

                update_diagnostics_statusbar(desktop, &state);

                Ok(AppControl::Continue)
            }
        },
        {
            let actions = actions.clone();
            let state = state.clone();

            move |desktop, event, screen, res| {
                handle_command_key_event(
                    desktop,
                    event,
                    screen,
                    res,
                    &mut command_keymap,
                    &state,
                    &actions,
                )
            }
        },
    )
}

fn build_menu(actions: EventQueue<AppAction>) -> MenuBar {
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

fn handle_command_key_event(
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

fn execute_command_action(
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

fn process_command_palette_events(
    desktop: &mut Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().command_palette_events.clone();
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_command_palette_focus(desktop, state);
                execute_command_action(desktop, state, actions, action);
            }
            PickerEvent::Submitted(_) => restore_command_palette_focus(desktop, state),
            PickerEvent::Closed => restore_command_palette_focus(desktop, state),
        }
    }
}

fn restore_command_palette_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.command_palette_window = None;
        s.command_palette_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

fn open_command_palette(desktop: &mut Desktop, screen: Rect, state: &Arc<Mutex<AppState>>) {
    if let Some(id) = state.lock().command_palette_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().command_palette_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.command_palette_events.clone();
        let _ = events.drain();
        s.command_palette_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new("Command Palette", command_palette_items(), events.clone())
        .placeholder("Type a command")
        .max_results(200);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 76, 18);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Command Palette", rect, Box::new(view))
            .with_tag("atto-editor-app-command-palette")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.command_palette_window == Some(id) {
                        s.command_palette_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().command_palette_window = Some(id);
}

fn command_palette_items() -> Vec<PickerItem<AppCommandAction>> {
    let registry = commands::app_command_registry();
    registry
        .commands()
        .iter()
        .map(|command| {
            let mut item = PickerItem::new(command.title.clone(), command.action.clone())
                .subtitle(command.category.clone());
            if let Some(sequence) = &command.default_sequence {
                item = item.shortcut(sequence.label());
            }
            item
        })
        .collect()
}

fn process_file_picker_events(
    desktop: &mut Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().file_picker_events.clone();
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_file_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(_) => restore_file_picker_focus(desktop, state),
            PickerEvent::Closed => restore_file_picker_focus(desktop, state),
        }
    }
}

fn restore_file_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.file_picker_window = None;
        s.file_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

fn open_file_picker(desktop: &mut Desktop, screen: Rect, state: &Arc<Mutex<AppState>>) {
    if let Some(id) = state.lock().file_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().file_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.file_picker_events.clone();
        let _ = events.drain();
        s.file_picker_restore_focus = desktop.wm.focused();
        events
    };
    let mut view = PickerView::new("File Picker", Vec::new(), events.clone())
        .placeholder("Type a file path")
        .max_results(300);
    match file_picker_cached_items(state) {
        // Cache hit: fill synchronously so the list is ready immediately.
        Some(items) => view.set_items(items),
        // Cache miss: build the index on a background thread to keep the UI
        // responsive; items stream in once ready.
        None => view = view.items_source(spawn_file_picker_index(state)),
    }
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 20);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "File Picker", rect, Box::new(view))
            .with_tag("atto-editor-app-file-picker")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.file_picker_window == Some(id) {
                        s.file_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().file_picker_window = Some(id);
}

/// Returns picker items from the cache when it matches the current workspace
/// roots, otherwise `None` (a background build is needed).
fn file_picker_cached_items(state: &Arc<Mutex<AppState>>) -> Option<Vec<PickerItem<AppAction>>> {
    let s = state.lock();
    let roots = canonical_workspace_roots(&s.workspace_roots);
    s.file_picker_cache
        .as_ref()
        .filter(|cache| cache.roots == roots)
        .map(file_picker_items_from_index)
}

/// Builds the workspace file index on a background thread and returns a receiver
/// the picker polls for the resulting items.
fn spawn_file_picker_index(state: &Arc<Mutex<AppState>>) -> Receiver<Vec<PickerItem<AppAction>>> {
    let (tx, rx) = mpsc::channel();
    let state = state.clone();
    thread::spawn(move || {
        let items = file_picker_items(&state);
        let _ = tx.send(items);
    });
    rx
}

fn file_picker_items(state: &Arc<Mutex<AppState>>) -> Vec<PickerItem<AppAction>> {
    let roots = {
        let s = state.lock();
        let roots = canonical_workspace_roots(&s.workspace_roots);
        if let Some(cache) = &s.file_picker_cache
            && cache.roots == roots
        {
            return file_picker_items_from_index(cache);
        }
        roots
    };

    let index = build_workspace_file_index(&roots, MAX_FILE_PICKER_ENTRIES);
    let items = file_picker_items_from_index(&index);
    {
        let mut s = state.lock();
        if canonical_workspace_roots(&s.workspace_roots) == index.roots {
            s.file_picker_cache = Some(index);
        } else {
            s.file_picker_cache = None;
        }
    }
    items
}

fn canonical_workspace_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .map(|root| canonicalize_best_effort(root))
        .collect()
}

fn file_picker_items_from_index(index: &WorkspaceFileIndex) -> Vec<PickerItem<AppAction>> {
    index
        .entries
        .iter()
        .map(|entry| {
            PickerItem::new(
                entry.display_path.clone(),
                AppAction::OpenPath {
                    path: entry.path.clone(),
                    target: OpenTarget::NewTab,
                },
            )
            .subtitle(entry.path.to_string_lossy())
        })
        .collect()
}

fn process_buffer_picker_events(
    desktop: &mut Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().buffer_picker_events.clone();
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_buffer_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(_) => restore_buffer_picker_focus(desktop, state),
            PickerEvent::Closed => restore_buffer_picker_focus(desktop, state),
        }
    }
}

fn restore_buffer_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.buffer_picker_window = None;
        s.buffer_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

fn open_buffer_picker(desktop: &mut Desktop, screen: Rect, state: &Arc<Mutex<AppState>>) {
    if let Some(id) = state.lock().buffer_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().buffer_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.buffer_picker_events.clone();
        let _ = events.drain();
        s.buffer_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new(
        "Buffer Picker",
        buffer_picker_items(desktop, state),
        events.clone(),
    )
    .placeholder("Type a buffer name")
    .max_results(200);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 18);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Buffer Picker", rect, Box::new(view))
            .with_tag("atto-editor-app-buffer-picker")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.buffer_picker_window == Some(id) {
                        s.buffer_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().buffer_picker_window = Some(id);
}

fn buffer_picker_items(
    desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
) -> Vec<PickerItem<AppAction>> {
    let summaries = state.lock().editor_tab_summaries.clone();
    let mut items = Vec::new();
    for window in desktop.wm.windows() {
        let window_id = window.id();
        let Some(tab_summaries) = summaries.get(&window_id) else {
            continue;
        };
        for summary in tab_summaries.get() {
            items.push(buffer_picker_item(window_id, summary));
        }
    }
    items
}

fn buffer_picker_item(window: WindowId, summary: EditorTabSummary) -> PickerItem<AppAction> {
    let title = if summary.dirty {
        format!("{}*", summary.title)
    } else {
        summary.title
    };
    let path = summary
        .path
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "[Untitled]".to_string());
    let active = if summary.active { "Active" } else { "Open" };

    PickerItem::new(
        title,
        AppAction::SelectEditorTab {
            window,
            tab_id: summary.tab_id,
        },
    )
    .subtitle(format!("{active} · Window {} · {path}", window.raw()))
}

fn process_document_symbol_picker_events(
    desktop: &mut Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().document_symbol_picker_events.clone();
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_document_symbol_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(_) => restore_document_symbol_picker_focus(desktop, state),
            PickerEvent::Closed => restore_document_symbol_picker_focus(desktop, state),
        }
    }
}

fn restore_document_symbol_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.document_symbol_picker_window = None;
        s.document_symbol_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

fn request_document_symbols(desktop: &Desktop, state: &Arc<Mutex<AppState>>) {
    if let Some(cmds) = active_editor_commands(desktop, state) {
        set_status_message(state, "Requesting document symbols…");
        cmds.push(EditorWindowCommand::RequestDocumentSymbols);
    } else {
        set_status_message(state, "No active editor for document symbols");
    }
}

fn open_document_symbol_results_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    outline: DocumentOutline,
) {
    if let Some(id) = state.lock().document_symbol_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().document_symbol_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.document_symbol_picker_events.clone();
        let _ = events.drain();
        s.document_symbol_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new(
        "Document Symbols",
        document_symbol_items(&outline),
        events.clone(),
    )
    .placeholder("Type a document symbol")
    .max_results(MAX_SYMBOL_PICKER_RESULTS);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 20);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Document Symbols", rect, Box::new(view))
            .with_tag("atto-editor-app-document-symbols")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.document_symbol_picker_window == Some(id) {
                        s.document_symbol_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().document_symbol_picker_window = Some(id);
}

fn document_symbol_items(outline: &DocumentOutline) -> Vec<PickerItem<AppAction>> {
    let mut items = Vec::new();
    for symbol in &outline.symbols {
        push_document_symbol_item(&mut items, symbol, 0);
    }
    items
}

fn push_document_symbol_item(
    items: &mut Vec<PickerItem<AppAction>>,
    symbol: &DocumentSymbol,
    depth: usize,
) {
    let title = format!("{}{}", "  ".repeat(depth), symbol.name);
    let mut subtitle = symbol_kind_label(symbol.kind).to_string();
    if let Some(detail) = &symbol.detail
        && !detail.is_empty()
    {
        subtitle.push_str(" · ");
        subtitle.push_str(detail);
    }
    items.push(
        PickerItem::new(
            title,
            AppAction::JumpTo(JumpTarget::CharOffset {
                offset: symbol.selection_range.start,
            }),
        )
        .subtitle(subtitle),
    );
    for child in &symbol.children {
        push_document_symbol_item(items, child, depth + 1);
    }
}

fn process_workspace_symbol_picker_events(
    desktop: &mut Desktop,
    _screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().workspace_symbol_picker_events.clone();
    let mut submitted_query = None;
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_workspace_symbol_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(query) => {
                restore_workspace_symbol_picker_focus(desktop, state);
                submitted_query = Some(query);
            }
            PickerEvent::Closed => restore_workspace_symbol_picker_focus(desktop, state),
        }
    }
    if let Some(query) = submitted_query {
        request_workspace_symbols(desktop, state, query);
    }
}

fn restore_workspace_symbol_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.workspace_symbol_picker_window = None;
        s.workspace_symbol_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

fn open_workspace_symbol_query_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
) {
    if let Some(id) = state.lock().workspace_symbol_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().workspace_symbol_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.workspace_symbol_picker_events.clone();
        let _ = events.drain();
        s.workspace_symbol_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::<AppAction>::new("Workspace Symbols", Vec::new(), events.clone())
        .placeholder("Type a symbol query and press Enter")
        .submit_query_on_empty(true);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 12);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Workspace Symbols", rect, Box::new(view))
            .with_tag("atto-editor-app-workspace-symbols")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.workspace_symbol_picker_window == Some(id) {
                        s.workspace_symbol_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().workspace_symbol_picker_window = Some(id);
}

fn request_workspace_symbols(
    _desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
    query: impl Into<String>,
) {
    let query = query.into();
    if query.trim().is_empty() {
        set_status_message(state, "Workspace symbol query is empty");
        return;
    }

    let workspace_state = state.lock().workspace_state.clone();
    let request = workspace_state
        .lock()
        .request_workspace_symbols(query.clone());
    match request {
        Ok(true) => set_status_message(
            state,
            format!("Requesting workspace symbols for “{query}”…"),
        ),
        Ok(false) => {
            set_status_message(
                state,
                "Workspace symbols require an active workspace LSP session",
            );
        }
        Err(err) => set_status_message(state, format!("Workspace symbol request failed: {err}")),
    }
}

fn open_workspace_symbol_results_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    symbols: Vec<WorkspaceSymbol>,
) {
    if let Some(id) = state.lock().workspace_symbol_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().workspace_symbol_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.workspace_symbol_picker_events.clone();
        let _ = events.drain();
        s.workspace_symbol_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new(
        "Workspace Symbols",
        workspace_symbol_items(&symbols),
        events.clone(),
    )
    .placeholder("Type to filter workspace symbols")
    .max_results(MAX_SYMBOL_PICKER_RESULTS);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 88, 20);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Workspace Symbols", rect, Box::new(view))
            .with_tag("atto-editor-app-workspace-symbols")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.workspace_symbol_picker_window == Some(id) {
                        s.workspace_symbol_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().workspace_symbol_picker_window = Some(id);
}

fn workspace_symbol_items(symbols: &[WorkspaceSymbol]) -> Vec<PickerItem<AppAction>> {
    symbols
        .iter()
        .map(|symbol| {
            let action = match editor_core_lsp::file_uri_to_path(&symbol.location.uri) {
                Some(path) => AppAction::OpenPathAndJump {
                    path,
                    target: JumpTarget::Utf16Position {
                        line: symbol.location.range.start.line,
                        character: symbol.location.range.start.character,
                    },
                },
                None => AppAction::ShowStatusMessage(format!(
                    "Unsupported workspace symbol URI: {}",
                    symbol.location.uri
                )),
            };
            let subtitle = workspace_symbol_subtitle(symbol);
            PickerItem::new(symbol.name.clone(), action).subtitle(subtitle)
        })
        .collect()
}

fn workspace_symbol_subtitle(symbol: &WorkspaceSymbol) -> String {
    let mut parts = vec![symbol_kind_label(symbol.kind).to_string()];
    if let Some(container) = &symbol.container_name
        && !container.is_empty()
    {
        parts.push(container.clone());
    }
    if let Some(detail) = &symbol.detail
        && !detail.is_empty()
    {
        parts.push(detail.clone());
    }
    parts.push(format!(
        "{}:{}:{}",
        symbol.location.uri,
        symbol.location.range.start.line.saturating_add(1),
        symbol.location.range.start.character.saturating_add(1)
    ));
    parts.join(" · ")
}

fn process_global_search_picker_events(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().global_search_picker_events.clone();
    let mut submitted_query = None;
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_global_search_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(query) => {
                restore_global_search_picker_focus(desktop, state);
                submitted_query = Some(query);
            }
            PickerEvent::Closed => restore_global_search_picker_focus(desktop, state),
        }
    }
    if let Some(query) = submitted_query {
        open_global_search_results_for_query(desktop, screen, state, query);
    }
}

fn restore_global_search_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.global_search_picker_window = None;
        s.global_search_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

fn open_global_search_query_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
) {
    if let Some(id) = state.lock().global_search_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().global_search_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.global_search_picker_events.clone();
        let _ = events.drain();
        s.global_search_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::<AppAction>::new("Global Search", Vec::new(), events.clone())
        .placeholder("Type search text and press Enter")
        .submit_query_on_empty(true);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 12);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Global Search", rect, Box::new(view))
            .with_tag("atto-editor-app-global-search")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.global_search_picker_window == Some(id) {
                        s.global_search_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().global_search_picker_window = Some(id);
}

fn open_global_search_results_for_query(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    query: String,
) {
    let roots = {
        let s = state.lock();
        canonical_workspace_roots(&s.workspace_roots)
    };
    if roots.is_empty() {
        set_status_message(state, "No workspace roots for global search");
        return;
    }

    let config = GlobalSearchConfig {
        max_total_matches: MAX_GLOBAL_SEARCH_RESULTS,
        ..GlobalSearchConfig::default()
    };
    match search_workspace(&roots, &query, SearchOptions::default(), config) {
        Ok(results) => {
            let count = results.len();
            open_global_search_results_picker(desktop, screen, state, &roots, results);
            set_status_message(state, format!("Global search found {count} result(s)"));
        }
        Err(err) => {
            set_status_message(state, format!("Global search failed: {err:#}"));
        }
    }
}

fn open_global_search_results_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    roots: &[PathBuf],
    results: Vec<GlobalSearchResult>,
) {
    if let Some(id) = state.lock().global_search_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().global_search_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.global_search_picker_events.clone();
        let _ = events.drain();
        s.global_search_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new(
        "Global Search",
        global_search_items(roots, &results),
        events.clone(),
    )
    .placeholder("Type to filter search results")
    .max_results(MAX_GLOBAL_SEARCH_RESULTS);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 90, 22);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Global Search", rect, Box::new(view))
            .with_tag("atto-editor-app-global-search")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.global_search_picker_window == Some(id) {
                        s.global_search_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().global_search_picker_window = Some(id);
}

fn global_search_items(
    roots: &[PathBuf],
    results: &[GlobalSearchResult],
) -> Vec<PickerItem<AppAction>> {
    results
        .iter()
        .map(|result| {
            let display_path = display_path_for_roots(&result.path, roots);
            PickerItem::new(
                format!("{}:{}:{}", display_path, result.line + 1, result.column + 1),
                AppAction::OpenPathAndJump {
                    path: result.path.clone(),
                    target: JumpTarget::CharPosition {
                        line: result.line,
                        column: result.column,
                    },
                },
            )
            .subtitle(result.text.trim().to_string())
        })
        .collect()
}

fn display_path_for_roots(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if let Ok(rel) = path.strip_prefix(root)
            && !rel.as_os_str().is_empty()
        {
            return rel.to_string_lossy().to_string();
        }
    }
    path.to_string_lossy().to_string()
}

fn process_workspace_lsp_events(desktop: &mut Desktop, screen: Rect, state: &Arc<Mutex<AppState>>) {
    let workspace_state = state.lock().workspace_state.clone();
    let (events, last_error) = {
        let mut workspace = workspace_state.lock();
        let events = workspace.poll_lsp();
        let last_error = workspace.take_last_error();
        (events, last_error)
    };

    if let Some(error) = last_error {
        set_status_message(state, error);
    }

    for event in events {
        match event {
            LspWorkspaceEvent::WorkspaceSymbols { query, symbols } => {
                let count = symbols.len();
                open_workspace_symbol_results_picker(desktop, screen, state, symbols);
                set_status_message(
                    state,
                    format!("Workspace symbols for “{query}”: {count} result(s)"),
                );
            }
            LspWorkspaceEvent::WorkspaceEditApplied { result } => {
                let applied = result.applied.len();
                if result.skipped_uris.is_empty() {
                    set_status_message(
                        state,
                        format!("Workspace edit applied to {applied} file(s)"),
                    );
                } else {
                    set_status_message(
                        state,
                        format!(
                            "Workspace edit applied to {applied} file(s), skipped {} unopened file(s)",
                            result.skipped_uris.len()
                        ),
                    );
                }
            }
            LspWorkspaceEvent::Message(message) => set_status_message(state, message),
        }
    }
}

fn process_editor_events(desktop: &mut Desktop, screen: Rect, state: &Arc<Mutex<AppState>>) {
    let queues = {
        let s = state.lock();
        s.editor_events
            .iter()
            .map(|(id, events)| (*id, events.clone()))
            .collect::<Vec<_>>()
    };

    for (window, events) in queues {
        if desktop.wm.window(window).is_none() {
            continue;
        }
        for event in events.drain() {
            match event {
                atto_ui_editor::EditorEvent::LspGoto { .. } => {}
                atto_ui_editor::EditorEvent::DocumentSymbols { outline } => {
                    open_document_symbol_results_picker(desktop, screen, state, outline);
                }
                atto_ui_editor::EditorEvent::WorkspaceSymbols { query: _, symbols } => {
                    open_workspace_symbol_results_picker(desktop, screen, state, symbols);
                }
                atto_ui_editor::EditorEvent::CodeActionMessage { message } => {
                    set_status_message(state, message);
                }
                atto_ui_editor::EditorEvent::LspRenameWorkspaceEdit { edit } => {
                    apply_rename_workspace_edit(state, edit);
                }
                atto_ui_editor::EditorEvent::LspMessage { message } => {
                    set_status_message(state, message);
                }
                atto_ui_editor::EditorEvent::FormatFinished { success, changed } => {
                    if success {
                        if changed {
                            set_status_message(state, "Formatted document");
                        } else {
                            set_status_message(state, "Format produced no changes");
                        }
                    }
                }
            }
        }
    }
}

fn apply_rename_workspace_edit(state: &Arc<Mutex<AppState>>, edit: serde_json::Value) {
    let workspace_state = state.lock().workspace_state.clone();
    let result = {
        let mut workspace = workspace_state.lock();
        if workspace.active_buffer_id().is_none() {
            set_status_message(state, "Rename requires workspace support");
            return;
        }
        workspace.apply_workspace_edit(&edit)
    };

    match result {
        Ok(result) => {
            let applied = result.applied.len();
            let skipped = result.skipped_uris.len();
            match (applied, skipped) {
                (0, 0) => set_status_message(state, "Rename produced no changes"),
                (0, _) => set_status_message(
                    state,
                    format!("Rename skipped {skipped} unopened file(s); no open files updated"),
                ),
                (_, 0) => {
                    set_status_message(state, format!("Rename applied to {applied} file(s)"));
                }
                (_, _) => set_status_message(
                    state,
                    format!(
                        "Rename applied to {applied} file(s), skipped {skipped} unopened file(s)"
                    ),
                ),
            }
        }
        Err(err) => set_status_message(state, format!("Rename failed: {err}")),
    }
}

fn symbol_kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::File => "File",
        SymbolKind::Module => "Module",
        SymbolKind::Namespace => "Namespace",
        SymbolKind::Package => "Package",
        SymbolKind::Class => "Class",
        SymbolKind::Method => "Method",
        SymbolKind::Property => "Property",
        SymbolKind::Field => "Field",
        SymbolKind::Constructor => "Constructor",
        SymbolKind::Enum => "Enum",
        SymbolKind::Interface => "Interface",
        SymbolKind::Function => "Function",
        SymbolKind::Variable => "Variable",
        SymbolKind::Constant => "Constant",
        SymbolKind::String => "String",
        SymbolKind::Number => "Number",
        SymbolKind::Boolean => "Boolean",
        SymbolKind::Array => "Array",
        SymbolKind::Object => "Object",
        SymbolKind::Key => "Key",
        SymbolKind::Null => "Null",
        SymbolKind::EnumMember => "EnumMember",
        SymbolKind::Struct => "Struct",
        SymbolKind::Event => "Event",
        SymbolKind::Operator => "Operator",
        SymbolKind::TypeParameter => "TypeParameter",
        SymbolKind::Custom(_) => "Custom",
    }
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

#[allow(clippy::too_many_arguments)]
fn open_path_with_jump(
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
        s.workspace_state
            .lock()
            .set_workspace_roots(s.workspace_roots.clone());
        s.file_picker_cache = None;
        (s.workspace_roots.clone(), s.explorer_commands.clone())
    };

    explorer_cmds.push(ExplorerWindowCommand::SetWorkspaceRoots(roots));
}

fn remove_editor_window_state(state: &Arc<Mutex<AppState>>, id: WindowId) {
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

fn update_diagnostics_statusbar(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let summary = active_editor_diagnostics_summary(desktop, state).unwrap_or_default();
    let editor_status = active_editor_status(desktop, state).unwrap_or_default();
    let status_message = state.lock().status_message.clone();
    desktop.status.set_segments(status_segments_for(
        desktop.mode,
        editor_status,
        summary,
        status_message,
    ));
}

fn set_status_message(state: &Arc<Mutex<AppState>>, message: impl Into<String>) {
    state.lock().status_message = Some(message.into());
}

fn status_left_for_mode(mode: DesktopMode) -> &'static str {
    match mode {
        DesktopMode::Normal => "F10 Menu  Ctrl+W Window  F6 Next",
        DesktopMode::Menu => "Menu: ←/→/↑/↓ Enter  Esc Close",
        DesktopMode::WindowManagement => {
            "Window: arrows move  Shift+arrows resize  c close  x max  m min  Esc exit"
        }
    }
}

fn format_diagnostics_summary(summary: atto_ui_editor::DiagnosticsSummary) -> String {
    format!("E:{} W:{}", summary.errors, summary.warnings)
}

fn status_segments_for(
    mode: DesktopMode,
    editor_status: EditorStatus,
    summary: atto_ui_editor::DiagnosticsSummary,
    status_message: Option<String>,
) -> Vec<StatusSegment> {
    let mut segments = vec![
        StatusSegment::new("app", "Atto Editor")
            .style("status-bar-key")
            .priority(100)
            .min_width(11),
        StatusSegment::new("path", status_path_text(editor_status.path.as_ref()))
            .priority(80)
            .min_width(8),
    ];

    if editor_status.dirty {
        segments.push(
            StatusSegment::new("dirty", "*")
                .style("status-segment-warning")
                .priority(90),
        );
    }

    if let Some(message) = status_message
        && !message.is_empty()
    {
        segments.push(
            StatusSegment::new("message", message)
                .style("status-segment-warning")
                .priority(95)
                .min_width(8),
        );
    }

    segments.push(
        StatusSegment::new("mode", status_left_for_mode(mode))
            .priority(10)
            .min_width(8),
    );

    let diagnostics_style = if summary.errors > 0 {
        "status-segment-error"
    } else if summary.warnings > 0 {
        "status-segment-warning"
    } else {
        "status-segment"
    };
    segments.push(
        StatusSegment::new("diagnostics", format_diagnostics_summary(summary))
            .style(diagnostics_style)
            .align(StatusSegmentAlign::Right)
            .priority(90)
            .min_width(7),
    );

    let language = if editor_status.language.is_empty() {
        "plaintext".to_string()
    } else {
        editor_status.language
    };
    segments.push(
        StatusSegment::new("language", language)
            .align(StatusSegmentAlign::Right)
            .priority(70)
            .min_width(4),
    );

    segments
}

fn status_path_text(path: Option<&PathBuf>) -> String {
    path.map(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    })
    .unwrap_or_else(|| "[No file]".to_string())
}

fn active_editor_diagnostics_summary(
    desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
) -> Option<atto_ui_editor::DiagnosticsSummary> {
    let focused = desktop.wm.focused();

    {
        let guard = state.lock();
        if let Some(id) = focused
            && desktop.wm.window(id).is_some()
            && let Some(summary) = guard.editor_diagnostics.get(&id)
        {
            return Some(summary.get());
        }

        if let Some(id) = guard.last_focused_editor
            && desktop.wm.window(id).is_some()
            && let Some(summary) = guard.editor_diagnostics.get(&id)
        {
            return Some(summary.get());
        }
    }

    let guard = state.lock();
    for w in desktop.wm.windows().iter().rev() {
        if let Some(summary) = guard.editor_diagnostics.get(&w.id()) {
            return Some(summary.get());
        }
    }

    None
}

fn active_editor_status(desktop: &Desktop, state: &Arc<Mutex<AppState>>) -> Option<EditorStatus> {
    let focused = desktop.wm.focused();

    {
        let guard = state.lock();
        if let Some(id) = focused
            && desktop.wm.window(id).is_some()
            && let Some(status) = guard.editor_statuses.get(&id)
        {
            return Some(status.get());
        }

        if let Some(id) = guard.last_focused_editor
            && desktop.wm.window(id).is_some()
            && let Some(status) = guard.editor_statuses.get(&id)
        {
            return Some(status.get());
        }
    }

    let guard = state.lock();
    for w in desktop.wm.windows().iter().rev() {
        if let Some(status) = guard.editor_statuses.get(&w.id()) {
            return Some(status.get());
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
) -> EventQueue<EditorWindowCommand> {
    let work = Desktop::layout(screen).work_area;
    let offset = state.lock().next_window_offset;
    state.lock().next_window_offset = offset.saturating_add(2);

    let rect = default_editor_rect(work, offset);
    let commands = EventQueue::<EditorWindowCommand>::new();
    let editor_events = EventQueue::<atto_ui_editor::EditorEvent>::new();
    let diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary> =
        atto_ui_editor::DiagnosticsSummary::default().into();
    let editor_status: Binding<EditorStatus> = EditorStatus::default().into();
    let tab_summaries: Binding<Vec<EditorTabSummary>> = Vec::new().into();
    let workspace_state = state.lock().workspace_state.clone();
    let view = EditorWindowView::new_with_workspace_bindings(
        actions,
        commands.clone(),
        editor_events.clone(),
        editor_theme,
        clipboard,
        workspace_state,
        EditorWindowBindings::new(
            diagnostics_summary.clone(),
            editor_status.clone(),
            tab_summaries.clone(),
        ),
    );
    let id = desktop.add_window(
        Window::new(WindowKind::Normal, "Atto Editor", rect, Box::new(view))
            .with_tag("atto-editor-app")
            .with_close_hook({
                let state = state.clone();
                move |id| {
                    remove_editor_window_state(&state, id);
                    true
                }
            }),
        screen,
    );
    {
        let mut s = state.lock();
        s.editor_windows.insert(id, commands.clone());
        s.editor_events.insert(id, editor_events);
        s.editor_diagnostics.insert(id, diagnostics_summary);
        s.editor_statuses.insert(id, editor_status);
        s.editor_tab_summaries.insert(id, tab_summaries);
        s.last_focused_editor = Some(id);
    }

    for file in initial_files {
        commands.push(EditorWindowCommand::OpenFile(file));
    }

    commands
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

    use crossterm::event::{
        Event, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("atto_editor_app_{prefix}_{nanos}"))
    }

    fn ctrl_alt_key(ch: char) -> Event {
        Event::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ))
    }

    fn ctrl_shift_key(ch: char) -> Event {
        Event::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ))
    }

    fn ctrl_key(ch: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL))
    }

    #[test]
    fn command_prefix_shows_which_key_and_exact_dispatches_action() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);
        let ignored = DesktopEventResult::ignored();

        handle_command_key_event(
            &mut desktop,
            &ctrl_alt_key('k'),
            screen,
            &ignored,
            &mut keymap,
            &state,
            &actions,
        )
        .expect("prefix handled");

        let popup = desktop.which_key().expect("which-key popup");
        assert_eq!(popup.prefix_label, commands::command_prefix().label());
        assert!(
            popup
                .choices
                .iter()
                .any(|choice| choice.command_id == "file.save" && choice.title == "Save")
        );

        handle_command_key_event(
            &mut desktop,
            &ctrl_alt_key('a'),
            screen,
            &ignored,
            &mut keymap,
            &state,
            &actions,
        )
        .expect("exact handled");

        assert!(desktop.which_key().is_none());
        assert_eq!(actions.drain(), vec![AppAction::Save]);
    }

    #[test]
    fn command_keymap_ignores_consumed_single_keys() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);
        let consumed = DesktopEventResult::consumed();

        handle_command_key_event(
            &mut desktop,
            &ctrl_alt_key('k'),
            screen,
            &consumed,
            &mut keymap,
            &state,
            &actions,
        )
        .expect("consumed key ignored");

        assert!(desktop.which_key().is_none());
        assert!(keymap.pending().is_empty());
        assert!(actions.is_empty());
    }

    #[test]
    fn command_palette_shortcut_dispatches_open_action() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);

        handle_command_key_event(
            &mut desktop,
            &ctrl_shift_key('p'),
            screen,
            &DesktopEventResult::ignored(),
            &mut keymap,
            &state,
            &actions,
        )
        .expect("command palette shortcut handled");

        assert_eq!(actions.drain(), vec![AppAction::OpenCommandPalette]);
    }

    #[test]
    fn file_picker_shortcut_dispatches_open_action() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);

        handle_command_key_event(
            &mut desktop,
            &ctrl_key('p'),
            screen,
            &DesktopEventResult::ignored(),
            &mut keymap,
            &state,
            &actions,
        )
        .expect("file picker shortcut handled");

        assert_eq!(actions.drain(), vec![AppAction::OpenFilePicker]);
    }

    #[test]
    fn global_search_shortcut_dispatches_open_action() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);

        handle_command_key_event(
            &mut desktop,
            &ctrl_shift_key('f'),
            screen,
            &DesktopEventResult::ignored(),
            &mut keymap,
            &state,
            &actions,
        )
        .expect("global search shortcut handled");

        assert_eq!(actions.drain(), vec![AppAction::OpenGlobalSearch]);
    }

    #[test]
    fn command_palette_items_come_from_command_registry() {
        let items = command_palette_items();

        let save = items
            .iter()
            .find(|item| item.title == "Save")
            .expect("save command item");
        assert_eq!(save.subtitle, "File");
        assert_eq!(save.shortcut.as_deref(), Some("Ctrl+Alt+K Ctrl+Alt+A"));
        assert!(items.iter().any(|item| item.title == "File Picker"));
        assert!(items.iter().any(|item| item.title == "Command Palette"));
        assert!(items.iter().any(|item| item.title == "Buffer Picker"));
        assert!(items.iter().any(|item| item.title == "Document Symbols"));
        assert!(items.iter().any(|item| item.title == "Workspace Symbols"));
        assert!(items.iter().any(|item| item.title == "Global Search"));
    }

    #[test]
    fn command_palette_close_restores_prior_focus() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Editor",
                Rect::new(4, 4, 40, 10),
                Box::new(TextFn::new(|| "editor".to_string())),
            ),
            screen,
        );
        assert_eq!(desktop.wm.focused(), Some(editor_id));

        open_command_palette(&mut desktop, screen, &state);
        let palette_id = state
            .lock()
            .command_palette_window
            .expect("command palette window");
        assert_eq!(desktop.wm.focused(), Some(palette_id));

        assert!(desktop.close_window(palette_id));
        process_command_palette_events(&mut desktop, &state, &actions);

        assert_eq!(desktop.wm.focused(), Some(editor_id));
    }

    #[test]
    fn command_prefix_escape_clears_pending_and_which_key() {
        let screen = Rect::new(0, 0, 80, 24);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), build_menu(actions.clone()));
        let mut keymap =
            commands::app_command_registry().key_sequence_engine(DEFAULT_KEY_SEQUENCE_TIMEOUT);
        let ignored = DesktopEventResult::ignored();

        handle_command_key_event(
            &mut desktop,
            &ctrl_alt_key('k'),
            screen,
            &ignored,
            &mut keymap,
            &state,
            &actions,
        )
        .expect("prefix handled");
        assert!(!keymap.pending().is_empty());
        assert!(desktop.which_key().is_some());

        handle_command_key_event(
            &mut desktop,
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            screen,
            &DesktopEventResult::consumed(),
            &mut keymap,
            &state,
            &actions,
        )
        .expect("escape handled");

        assert!(keymap.pending().is_empty());
        assert!(desktop.which_key().is_none());
        assert!(actions.is_empty());
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
            atto_ui_editor::DiagnosticsSummary::default().into(),
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
    fn file_picker_items_rebuild_when_workspace_roots_change() {
        let root_a = unique_temp_dir("file_picker_root_a");
        let root_b = unique_temp_dir("file_picker_root_b");
        fs::create_dir_all(root_a.join("src")).expect("create root a");
        fs::create_dir_all(root_b.join("src")).expect("create root b");
        fs::write(root_a.join("src").join("main.rs"), "fn main() {}\n").expect("write main");
        fs::write(root_b.join("src").join("lib.rs"), "pub fn lib() {}\n").expect("write lib");

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        state.lock().workspace_roots = vec![root_a.clone()];

        let first = file_picker_items(&state);
        assert!(first.iter().any(|item| item.title == "src/main.rs"));
        assert!(!first.iter().any(|item| item.title == "src/lib.rs"));

        state.lock().workspace_roots = vec![root_b.clone()];
        let second = file_picker_items(&state);

        assert!(second.iter().any(|item| item.title == "src/lib.rs"));
        assert!(!second.iter().any(|item| item.title == "src/main.rs"));
        assert_eq!(
            state
                .lock()
                .file_picker_cache
                .as_ref()
                .map(|cache| cache.roots.clone()),
            Some(vec![canonicalize_best_effort(&root_b)])
        );
    }

    #[test]
    fn document_symbol_items_jump_to_selection_offset() {
        let outline = DocumentOutline::new(vec![DocumentSymbol {
            name: "main".to_string(),
            detail: Some("fn()".to_string()),
            kind: SymbolKind::Function,
            range: editor_core::SymbolRange::new(0, 10),
            selection_range: editor_core::SymbolRange::new(3, 7),
            children: Vec::new(),
            data_json: None,
        }]);

        let items = document_symbol_items(&outline);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "main");
        assert_eq!(
            items[0].action,
            AppAction::JumpTo(JumpTarget::CharOffset { offset: 3 })
        );
    }

    #[test]
    fn workspace_symbol_items_reject_non_file_uris_explicitly() {
        let symbols = vec![WorkspaceSymbol {
            name: "remote".to_string(),
            detail: None,
            kind: SymbolKind::Function,
            location: editor_core::SymbolLocation {
                uri: "untitled://remote".to_string(),
                range: editor_core::Utf16Range::new(
                    editor_core::Utf16Position::new(2, 4),
                    editor_core::Utf16Position::new(2, 10),
                ),
            },
            container_name: None,
            data_json: None,
        }];

        let items = workspace_symbol_items(&symbols);

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0].action,
            AppAction::ShowStatusMessage(message)
                if message.contains("Unsupported workspace symbol URI")
        ));
    }

    #[test]
    fn global_search_items_open_file_and_jump_to_match() {
        let root = PathBuf::from("/tmp/workspace");
        let result = GlobalSearchResult {
            path: root.join("src/main.rs"),
            line: 4,
            column: 8,
            text: "let todo = \"TODO\";".to_string(),
            ranges: Vec::new(),
        };

        let items = global_search_items(std::slice::from_ref(&root), std::slice::from_ref(&result));

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "src/main.rs:5:9");
        assert_eq!(
            items[0].action,
            AppAction::OpenPathAndJump {
                path: result.path,
                target: JumpTarget::CharPosition { line: 4, column: 8 },
            }
        );
    }

    #[test]
    fn buffer_picker_selects_tab_by_stable_id_after_close() -> anyhow::Result<()> {
        let root = unique_temp_dir("buffer_picker_tabs");
        fs::create_dir_all(&root)?;
        let first = root.join("one.rs");
        let second = root.join("two.rs");
        let third = root.join("three.rs");
        fs::write(&first, "// one\n")?;
        fs::write(&second, "// two\n")?;
        fs::write(&third, "// three\n")?;

        let screen = Rect::new(0, 0, 90, 28);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(Vec::new()));
        let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: atto_ui::reactive::Binding<String> = String::new().into();
        let commands = EventQueue::<EditorWindowCommand>::new();
        let editor_events = EventQueue::<atto_ui_editor::EditorEvent>::new();
        let diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary> =
            atto_ui_editor::DiagnosticsSummary::default().into();
        let editor_status: Binding<EditorStatus> = EditorStatus::default().into();
        let tab_summaries: Binding<Vec<EditorTabSummary>> = Vec::new().into();
        let editor = EditorWindowView::new_with_bindings(
            actions.clone(),
            commands.clone(),
            editor_events,
            editor_theme.clone(),
            clipboard.clone(),
            EditorWindowBindings::new(
                diagnostics_summary.clone(),
                editor_status.clone(),
                tab_summaries.clone(),
            ),
        );
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
            s.editor_windows.insert(editor_id, commands.clone());
            s.editor_diagnostics.insert(editor_id, diagnostics_summary);
            s.editor_statuses.insert(editor_id, editor_status);
            s.editor_tab_summaries
                .insert(editor_id, tab_summaries.clone());
            s.last_focused_editor = Some(editor_id);
        }

        commands.push(EditorWindowCommand::OpenFile(first.clone()));
        commands.push(EditorWindowCommand::OpenFile(second));
        commands.push(EditorWindowCommand::OpenFile(third));
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| desktop.draw(f))?;

        let summaries = tab_summaries.get();
        assert_eq!(summaries.len(), 3);
        let first_tab_id = summaries[0].tab_id;
        let second_tab_id = summaries[1].tab_id;

        commands.push(EditorWindowCommand::CloseActiveTab);
        terminal.draw(|f| desktop.draw(f))?;
        let summaries_after_close = tab_summaries.get();
        assert_eq!(summaries_after_close.len(), 2);
        assert_eq!(summaries_after_close[0].tab_id, first_tab_id);
        assert_eq!(summaries_after_close[1].tab_id, second_tab_id);

        let item = buffer_picker_items(&desktop, &state)
            .into_iter()
            .find(|item| item.title == "one.rs")
            .expect("first tab picker item");
        assert_eq!(
            item.action,
            AppAction::SelectEditorTab {
                window: editor_id,
                tab_id: first_tab_id,
            }
        );

        handle_action(
            &mut desktop,
            screen,
            &state,
            &actions,
            item.action,
            &Property::new(None),
            &Property::new(None),
            &Property::new(String::new()),
            editor_theme,
            clipboard,
        )?;
        terminal.draw(|f| desktop.draw(f))?;

        let active = tab_summaries
            .get()
            .into_iter()
            .find(|summary| summary.active)
            .expect("active tab summary");
        assert_eq!(active.tab_id, first_tab_id);
        assert_eq!(active.path, Some(canonicalize_best_effort(&first)));

        Ok(())
    }

    #[test]
    fn workspace_edit_marks_open_tab_dirty() -> anyhow::Result<()> {
        let root = unique_temp_dir("workspace_edit_dirty");
        fs::create_dir_all(&root)?;
        let file = root.join("main.rs");
        fs::write(&file, "hello world\n")?;
        let file = canonicalize_best_effort(&file);

        let screen = Rect::new(0, 0, 90, 28);
        let actions: EventQueue<AppAction> = EventQueue::new();
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(Vec::new()));
        let editor_theme: Binding<atto_ui_editor::EditorThemeSet> =
            atto_ui_editor::EditorThemeSet::default().into();
        let clipboard: Binding<String> = String::new().into();
        let commands = EventQueue::<EditorWindowCommand>::new();
        let editor_events = EventQueue::<atto_ui_editor::EditorEvent>::new();
        let diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary> =
            atto_ui_editor::DiagnosticsSummary::default().into();
        let editor_status: Binding<EditorStatus> = EditorStatus::default().into();
        let tab_summaries: Binding<Vec<EditorTabSummary>> = Vec::new().into();
        let workspace_state = WorkspaceState::shared();
        workspace_state
            .lock()
            .set_workspace_roots(vec![canonicalize_best_effort(&root)]);
        let editor = EditorWindowView::new_with_workspace_bindings(
            actions,
            commands.clone(),
            editor_events,
            editor_theme,
            clipboard,
            workspace_state.clone(),
            EditorWindowBindings::new(diagnostics_summary, editor_status, tab_summaries.clone()),
        );
        desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Atto Editor",
                default_editor_rect(Desktop::layout(screen).work_area, 0),
                Box::new(editor),
            ),
            screen,
        );

        commands.push(EditorWindowCommand::OpenFile(file.clone()));
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| desktop.draw(f))?;
        assert_eq!(tab_summaries.get().len(), 1);
        assert!(!tab_summaries.get()[0].dirty);

        let mut changes = serde_json::Map::new();
        changes.insert(
            editor_core_lsp::path_to_file_uri(&file),
            serde_json::json!([
                {
                    "range": {
                        "start": { "line": 0, "character": 6 },
                        "end": { "line": 0, "character": 11 }
                    },
                    "newText": "atto"
                }
            ]),
        );
        workspace_state
            .lock()
            .apply_workspace_edit(&serde_json::json!({ "changes": changes }))
            .map_err(anyhow::Error::msg)?;

        terminal.draw(|f| desktop.draw(f))?;
        let summaries = tab_summaries.get();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].dirty);
        assert!(workspace_state.lock().take_last_error().is_none());

        Ok(())
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
        let editor = EditorWindowView::new(
            actions,
            editor_commands,
            editor_theme,
            clipboard,
            atto_ui_editor::DiagnosticsSummary::default().into(),
        );
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
        let editor = EditorWindowView::new(
            actions,
            editor_commands.clone(),
            editor_theme,
            clipboard,
            atto_ui_editor::DiagnosticsSummary::default().into(),
        );
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
    fn diagnostics_statusbar_uses_last_focused_editor_when_explorer_is_focused() {
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
        let diagnostics_summary: atto_ui::reactive::Binding<atto_ui_editor::DiagnosticsSummary> =
            atto_ui_editor::DiagnosticsSummary {
                errors: 1,
                warnings: 2,
                infos: 3,
                hints: 4,
            }
            .into();
        let editor_status: atto_ui::reactive::Binding<EditorStatus> = EditorStatus {
            path: Some(PathBuf::from("src/main.rs")),
            language: "rust".to_string(),
            dirty: false,
        }
        .into();
        let editor = EditorWindowView::new_with_status(
            actions,
            editor_commands.clone(),
            EventQueue::<atto_ui_editor::EditorEvent>::new(),
            editor_theme,
            clipboard,
            diagnostics_summary.clone(),
            editor_status.clone(),
        );
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
            s.editor_windows.insert(editor_id, editor_commands);
            s.editor_diagnostics.insert(editor_id, diagnostics_summary);
            s.editor_statuses.insert(editor_id, editor_status);
            s.last_focused_editor = Some(editor_id);
        }

        desktop.wm.focus(explorer_id);
        update_diagnostics_statusbar(&mut desktop, &state);

        let backend = TestBackend::new(90, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| {
                desktop
                    .status
                    .draw(f, Rect::new(0, 0, 90, 1), &desktop.theme)
            })
            .expect("draw statusbar");

        let mut row = String::new();
        let buf = terminal.backend().buffer();
        for x in 0..90 {
            row.push_str(buf[(x, 0)].symbol());
        }
        assert!(
            row.contains("E:1 W:2"),
            "expected diagnostics summary in statusbar, got {row:?}"
        );
        assert!(
            row.contains("rust"),
            "expected language in statusbar, got {row:?}"
        );
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
            atto_ui_editor::DiagnosticsSummary::default().into(),
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
