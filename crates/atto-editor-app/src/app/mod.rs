//! Editor application entry point and orchestration.
//!
//! [`run`] wires a [`Desktop`] with editor windows, a dockable explorer, pickers
//! (file / buffer / symbol / global-search / command palette), and an LSP
//! workspace. The supporting free functions are grouped into submodules:
//!
//! - `menu`: menu-bar construction.
//! - `dispatch`: `AppAction` / command-palette dispatch.
//! - `picker/`: the six pickers (command / file / buffer / document-symbol /
//!   workspace-symbol / global-search).
//! - `events`: workspace-LSP and editor event fan-out.
//! - `paths`: open-path and workspace-root management.
//! - `explorer`: explorer dock management.
//! - `window`: editor-focus resolution.
//! - `status`: status-bar segment formatting.
//! - `layout`: window placement and startup helpers.

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

mod dispatch;
mod events;
mod explorer;
mod layout;
mod menu;
mod paths;
mod picker;
mod status;
mod window;

// Flatten submodule free functions into this scope so the run-loop closures
// resolve them exactly as before the split. All moved items are `pub(crate)`.
use dispatch::*;
use events::*;
use explorer::*;
use layout::*;
use menu::*;
use paths::*;
use picker::*;
use status::*;
use window::*;

#[cfg(test)]
mod tests;

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
