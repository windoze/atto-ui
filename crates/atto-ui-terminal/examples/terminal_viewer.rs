use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    run_crossterm_desktop_with_actions,
};
use atto_ui::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout, Scrollable,
};
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind, WindowState};
use atto_ui_terminal::{
    TerminalCommandBlockPresentation, TerminalConfig, TerminalEmulator, TerminalHandle,
    TerminalPaneGroup, TerminalPaneGroupHandle, TerminalPaneId, TerminalSessionSpec,
    TerminalSettingsView, TerminalShortcut, default_terminal_config_path,
    load_terminal_config_or_default,
};

const WINDOWS_MENU_ID: &str = "atto-ui-terminal:terminal_viewer:windows";
const WINDOWS_MENU_LIST_ID: &str = "atto-ui-terminal:terminal_viewer:windows:list";

#[derive(Clone, Copy, Debug)]
enum TerminalViewerAction {
    NewShellWindow,
    NewCommandWindow,
    OpenSettings,
    Quit,
    FocusNext,
    MinimizeFocused,
    ToggleMaximizeFocused,
    CloseFocused,
    FocusWindow(WindowId),
    CommandContext(CommandContextMenuAction),
}

#[derive(Clone, Copy, Debug)]
enum CommandContextMenuAction {
    Rerun,
    CopyCommand,
    CopyOutput,
}

struct TerminalWindowSession {
    id: WindowId,
    panes: TerminalPaneGroupHandle,
    window_number: usize,
    spec: TerminalSessionSpec,
    exit_prompted: bool,
}

impl TerminalWindowSession {
    fn new(
        id: WindowId,
        panes: TerminalPaneGroupHandle,
        window_number: usize,
        spec: TerminalSessionSpec,
    ) -> Self {
        Self {
            id,
            panes,
            window_number,
            spec,
            exit_prompted: false,
        }
    }

    fn active_handle(&self) -> Option<TerminalHandle> {
        self.panes.active_terminal_handle()
    }
}

#[derive(Clone, Copy, Debug)]
struct CommandContextState {
    menu_id: WindowId,
    terminal_id: WindowId,
    pane_id: TerminalPaneId,
    block_index: usize,
}

struct CommandContextMenuView {
    action_tx: mpsc::Sender<TerminalViewerAction>,
    last_area: Option<Rect>,
}

impl CommandContextMenuView {
    fn new(action_tx: mpsc::Sender<TerminalViewerAction>) -> Self {
        Self {
            action_tx,
            last_area: None,
        }
    }

    fn action_for_row(row: u16) -> Option<CommandContextMenuAction> {
        match row {
            0 => Some(CommandContextMenuAction::Rerun),
            1 => Some(CommandContextMenuAction::CopyCommand),
            2 => Some(CommandContextMenuAction::CopyOutput),
            _ => None,
        }
    }
}

impl Component for CommandContextMenuView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("Rerun"),
                Line::from("Copy command"),
                Line::from("Copy output"),
            ]),
            area,
        );
    }
}

impl EventHandling for CommandContextMenuView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            ..
        }) = event
        else {
            return EventResult::ignored();
        };
        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };
        let (local_col, local_row) = match ctx.mouse_coordinate_space {
            atto_ui::composable::MouseCoordinateSpace::Absolute => {
                if *column < area.x
                    || *row < area.y
                    || *column >= area.x.saturating_add(area.width)
                    || *row >= area.y.saturating_add(area.height)
                {
                    return EventResult::ignored();
                }
                (
                    (*column).saturating_sub(area.x),
                    (*row).saturating_sub(area.y),
                )
            }
            atto_ui::composable::MouseCoordinateSpace::Local => (*column, *row),
        };
        if local_col >= area.width {
            return EventResult::ignored();
        }
        let Some(action) = Self::action_for_row(local_row) else {
            return EventResult::ignored();
        };
        let _ = self
            .action_tx
            .send(TerminalViewerAction::CommandContext(action));
        EventResult::consumed()
    }
}

impl Layout for CommandContextMenuView {}
impl Scrollable for CommandContextMenuView {}
impl FocusNav for CommandContextMenuView {}
atto_ui::impl_component_default_traits!(CommandContextMenuView => DynamicTree);

fn build_menu(action_tx: mpsc::Sender<TerminalViewerAction>) -> MenuBar {
    MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::action("New shell window", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::NewShellWindow);
                    }
                })
                .shortcut("n"),
                MenuItem::action("New command window", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::NewCommandWindow);
                    }
                })
                .shortcut("c"),
                MenuItem::action("Settings", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::OpenSettings);
                    }
                })
                .shortcut(","),
                MenuItem::action("Quit", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::Quit);
                    }
                })
                .shortcut("q"),
            ],
        ),
        MenuSpec::new(
            "Windows",
            vec![
                MenuItem::action("Next window", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::FocusNext);
                    }
                })
                .shortcut("F6"),
                MenuItem::action("Minimize focused", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::MinimizeFocused);
                    }
                })
                .shortcut("m"),
                MenuItem::action("Maximize/restore focused", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::ToggleMaximizeFocused);
                    }
                })
                .shortcut("x"),
                MenuItem::action("Close focused", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::CloseFocused);
                    }
                })
                .shortcut("w"),
                MenuItem::minimized_windows("Minimized windows"),
                MenuItem::submenu("Switch to", Vec::new()).with_tag(WINDOWS_MENU_LIST_ID),
            ],
        )
        .with_tag(WINDOWS_MENU_ID),
    ])
}

fn terminal_release_shortcut() -> TerminalShortcut {
    TerminalShortcut::new(
        KeyCode::Char('l'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )
}

fn terminal_window_rect(work_area: Rect, index: usize) -> Rect {
    let base_width = work_area.width.saturating_sub(6).clamp(30, 110);
    let base_height = work_area.height.saturating_sub(6).clamp(10, 32);

    let index = index.max(1) as u16;
    let offset_x = (index.saturating_sub(1) * 2) % 10;
    let offset_y = (index.saturating_sub(1)) % 6;

    Rect {
        x: work_area.x.saturating_add(2 + offset_x),
        y: work_area.y.saturating_add(1 + offset_y),
        width: base_width,
        height: base_height,
    }
}

fn terminal_window_title(window_number: usize) -> String {
    format!("Terminal {window_number}")
}

fn is_plain_restart_key(event: &Event) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
        return false;
    }
    matches!(key.code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&'r'))
}

fn seed_terminal_banner(handle: &TerminalHandle, window_number: usize, pane_number: usize) {
    let banner = format!("Terminal Emulator ({window_number}.{pane_number})\r\n");
    handle.process_output_str(&banner);
    handle
        .process_output_str("Menu: click the menu bar, or press F10 after releasing capture.\r\n");
    handle.process_output_str("Ctrl+Shift+L: release capture; click terminal to recapture.\r\n");
    handle.process_output_str(
        "Ctrl+B %: split right; Ctrl+B \" split below; Ctrl+B o: next pane.\r\n",
    );
    handle.process_output_str("\x1b[?1000h\x1b[?1006h");
}

fn build_terminal_view(
    window_number: usize,
    pane_number: usize,
    spec: &TerminalSessionSpec,
) -> Result<(TerminalEmulator, TerminalHandle)> {
    let mut terminal = TerminalEmulator::new()
        .release_shortcut(terminal_release_shortcut())
        .command_block_presentation(TerminalCommandBlockPresentation::enabled());
    terminal.spawn_session(spec)?;
    let handle = terminal.handle();
    seed_terminal_banner(&handle, window_number, pane_number);
    Ok((terminal, handle))
}

fn build_terminal_pane_group(
    window_number: usize,
    spec: &TerminalSessionSpec,
) -> Result<(TerminalPaneGroup, TerminalPaneGroupHandle)> {
    let (terminal, _) = build_terminal_view(window_number, 1, spec)?;
    let spec = spec.clone();
    let group = TerminalPaneGroup::new(terminal).pane_factory(move |pane_number| {
        build_terminal_view(window_number, pane_number, &spec).map(|(terminal, _)| terminal)
    });
    let handle = group.handle();
    Ok((group, handle))
}

fn prune_terminal_sessions(desktop: &Desktop, sessions: &mut Vec<TerminalWindowSession>) {
    sessions.retain(|session| desktop.wm.windows().iter().any(|w| w.id() == session.id));
}

fn sync_session_cwd_from_active_pane(session: &mut TerminalWindowSession) -> bool {
    let Some(cwd) = session
        .active_handle()
        .and_then(|handle| handle.current_cwd())
    else {
        return false;
    };
    let cwd = PathBuf::from(cwd);
    if session.spec.cwd() == Some(cwd.as_path()) {
        return false;
    }
    session.spec.set_cwd(cwd);
    true
}

fn sync_session_cwds(sessions: &mut [TerminalWindowSession]) -> bool {
    let mut changed = false;
    for session in sessions {
        changed |= sync_session_cwd_from_active_pane(session);
    }
    changed
}

fn focused_session_cwd(desktop: &Desktop, sessions: &[TerminalWindowSession]) -> Option<PathBuf> {
    let focused = desktop.wm.focused()?;
    sessions
        .iter()
        .find(|session| session.id == focused)
        .and_then(|session| session.spec.cwd().map(PathBuf::from))
}

fn spec_for_new_window(
    desktop: &Desktop,
    sessions: &[TerminalWindowSession],
    base: &TerminalSessionSpec,
) -> TerminalSessionSpec {
    let mut spec = base.clone();
    if let Some(cwd) = focused_session_cwd(desktop, sessions) {
        spec.set_cwd(cwd);
    }
    spec
}

fn show_exit_prompt_if_needed(session: &mut TerminalWindowSession) -> bool {
    if session.exit_prompted {
        return false;
    }
    let Some(handle) = session.active_handle() else {
        return false;
    };
    let Some(status) = handle.exit_status() else {
        return false;
    };

    handle.set_capture(false);
    handle.process_output_str(&format!(
        "\r\n[Process exited: code {} — press R to restart]\r\n",
        status.exit_code()
    ));
    session.exit_prompted = true;
    true
}

fn update_terminal_exit_prompts(
    desktop: &Desktop,
    sessions: &mut Vec<TerminalWindowSession>,
) -> bool {
    prune_terminal_sessions(desktop, sessions);
    let mut changed = false;
    for session in sessions {
        changed |= show_exit_prompt_if_needed(session);
    }
    changed
}

fn sync_terminal_window_titles(desktop: &mut Desktop, sessions: &[TerminalWindowSession]) -> bool {
    let mut changed = false;
    for session in sessions {
        let Some(handle) = session.active_handle() else {
            continue;
        };
        let Some(title) = handle.window_title() else {
            continue;
        };
        let current_title = desktop.wm.window(session.id).map(|w| w.title.get());
        if current_title.as_deref() == Some(title.as_str()) {
            continue;
        }
        changed |= desktop.set_title(session.id, title);
    }
    changed
}

fn restart_terminal_window(
    desktop: &mut Desktop,
    session: &mut TerminalWindowSession,
) -> Result<bool> {
    if !desktop.wm.windows().iter().any(|w| w.id() == session.id) {
        return Ok(false);
    }

    let (terminal, panes) = build_terminal_pane_group(session.window_number, &session.spec)?;
    if !desktop.wm.set_view(session.id, Box::new(terminal)) {
        return Ok(false);
    }
    session.panes = panes;
    session.exit_prompted = false;
    desktop.set_title(session.id, terminal_window_title(session.window_number));
    desktop.wm.focus(session.id);
    Ok(true)
}

fn restart_focused_terminal(
    desktop: &mut Desktop,
    sessions: &mut [TerminalWindowSession],
) -> Result<bool> {
    if desktop.menu.is_active() {
        return Ok(false);
    }
    let Some(focused) = desktop.wm.focused() else {
        return Ok(false);
    };
    let Some(session) = sessions
        .iter_mut()
        .find(|session| session.id == focused && session.exit_prompted)
    else {
        return Ok(false);
    };
    restart_terminal_window(desktop, session)
}

fn close_command_context_menu(desktop: &mut Desktop, context: &mut Option<CommandContextState>) {
    if let Some(context) = context.take() {
        desktop.wm.close(context.menu_id);
    }
}

fn command_block_at_mouse(
    desktop: &Desktop,
    sessions: &[TerminalWindowSession],
    mouse: &MouseEvent,
) -> Option<(WindowId, TerminalPaneId, usize)> {
    for session in sessions {
        let inner = desktop.wm.window(session.id)?.inner_rect();
        if mouse.column < inner.x
            || mouse.row < inner.y
            || mouse.column >= inner.x.saturating_add(inner.width)
            || mouse.row >= inner.y.saturating_add(inner.height)
        {
            continue;
        }
        let Some(pane) = session
            .panes
            .pane_at_screen_position(mouse.column, mouse.row)
        else {
            continue;
        };
        let Some(rect) = pane.rect else {
            continue;
        };
        let row = mouse.row.saturating_sub(rect.y);
        let col = mouse.column.saturating_sub(rect.x);
        let position = pane.handle.selection_position_for_view_cell(row, col);
        if let Some(block_index) = pane.handle.command_block_index_at_position(position) {
            return Some((session.id, pane.id, block_index));
        }
    }
    None
}

fn command_context_menu_rect(screen: Rect, mouse: &MouseEvent) -> Rect {
    let width = 18.min(screen.width.max(1));
    let height = 5.min(screen.height.max(1));
    Rect {
        x: mouse.column.min(screen.width.saturating_sub(width)),
        y: mouse.row.min(screen.height.saturating_sub(height)),
        width,
        height,
    }
}

fn open_command_context_menu(
    desktop: &mut Desktop,
    sessions: &[TerminalWindowSession],
    screen: Rect,
    mouse: &MouseEvent,
    action_tx: &mpsc::Sender<TerminalViewerAction>,
    context: &mut Option<CommandContextState>,
) -> bool {
    close_command_context_menu(desktop, context);
    let Some((terminal_id, pane_id, block_index)) =
        command_block_at_mouse(desktop, sessions, mouse)
    else {
        return false;
    };
    if let Some(session) = sessions.iter().find(|session| session.id == terminal_id)
        && let Some(pane) = session
            .panes
            .panes()
            .into_iter()
            .find(|pane| pane.id == pane_id)
    {
        let _ = pane.handle.select_command_block_output(block_index);
    }
    let menu_id = desktop.add_window(
        Window::new(
            WindowKind::Tooltip,
            "Command",
            command_context_menu_rect(screen, mouse),
            Box::new(CommandContextMenuView::new(action_tx.clone())),
        )
        .with_min_size(18, 5),
        screen,
    );
    *context = Some(CommandContextState {
        menu_id,
        terminal_id,
        pane_id,
        block_index,
    });
    true
}

fn apply_command_context_action(
    desktop: &mut Desktop,
    sessions: &[TerminalWindowSession],
    context: &mut Option<CommandContextState>,
    action: CommandContextMenuAction,
) {
    let Some(active) = *context else {
        return;
    };
    let Some(session) = sessions
        .iter()
        .find(|session| session.id == active.terminal_id)
    else {
        close_command_context_menu(desktop, context);
        return;
    };
    let Some(pane) = session
        .panes
        .panes()
        .into_iter()
        .find(|pane| pane.id == active.pane_id)
    else {
        close_command_context_menu(desktop, context);
        return;
    };
    match action {
        CommandContextMenuAction::Rerun => {
            pane.handle.rerun_command_block(active.block_index);
        }
        CommandContextMenuAction::CopyCommand => {
            pane.handle.copy_command_block_command(active.block_index);
        }
        CommandContextMenuAction::CopyOutput => {
            pane.handle.copy_command_block_output(active.block_index);
        }
    }
    close_command_context_menu(desktop, context);
}

fn is_right_mouse_down(event: &Event) -> Option<&MouseEvent> {
    match event {
        Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right)) => {
            Some(mouse)
        }
        _ => None,
    }
}

fn spawn_terminal_window(
    desktop: &mut Desktop,
    screen: Rect,
    window_number: usize,
    spec: TerminalSessionSpec,
) -> Result<TerminalWindowSession> {
    let work_area = Desktop::layout(screen).work_area;
    let rect = terminal_window_rect(work_area, window_number);

    let (terminal, panes) = build_terminal_pane_group(window_number, &spec)?;
    let title = terminal_window_title(window_number);
    let window = Window::new(WindowKind::Normal, title, rect, Box::new(terminal));
    let id = desktop.add_window(window, screen);
    Ok(TerminalWindowSession::new(id, panes, window_number, spec))
}

fn settings_window_rect(screen: Rect) -> Rect {
    let work_area = Desktop::layout(screen).work_area;
    let width = 76.min(work_area.width.saturating_sub(4)).max(40);
    let height = 22.min(work_area.height.saturating_sub(2)).max(12);
    Rect {
        x: work_area.x + work_area.width.saturating_sub(width) / 2,
        y: work_area.y + work_area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn open_settings_window(
    desktop: &mut Desktop,
    screen: Rect,
    config: Binding<TerminalConfig>,
    config_path: Option<PathBuf>,
    settings_window_id: &mut Option<WindowId>,
) {
    if let Some(id) = *settings_window_id
        && desktop.wm.restore_window(id)
    {
        desktop.wm.focus(id);
        return;
    }

    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Terminal Settings",
            settings_window_rect(screen),
            Box::new(TerminalSettingsView::new(config, config_path)),
        )
        .with_min_size(40, 12),
        screen,
    );
    desktop.wm.focus(id);
    *settings_window_id = Some(id);
}

fn refresh_windows_menu(desktop: &mut Desktop, action_tx: &mpsc::Sender<TerminalViewerAction>) {
    if desktop.menu.is_active() {
        return;
    }

    let focused = desktop.wm.focused();
    let mut windows: Vec<(String, WindowId, bool)> = desktop
        .wm
        .windows()
        .iter()
        .filter(|w| w.kind.is_focusable())
        .filter(|w| w.state.get() != WindowState::Minimized)
        .map(|w| {
            let title = w.title.get();
            let id = w.id();
            let is_focused = Some(id) == focused;
            (title, id, is_focused)
        })
        .collect();
    windows.sort_by(|a, b| a.0.cmp(&b.0));

    let mut items = Vec::new();
    for (title, id, is_focused) in windows {
        let label = if is_focused {
            format!("* {title}")
        } else {
            title
        };
        let action_tx = action_tx.clone();
        items.push(MenuItem::action(label, move || {
            let _ = action_tx.send(TerminalViewerAction::FocusWindow(id));
        }));
    }
    if items.is_empty() {
        items.push(MenuItem::submenu("No terminal windows", Vec::new()).enabled(false));
    }

    for menu in desktop.menu.menus_mut() {
        if menu.tag.as_deref() != Some(WINDOWS_MENU_ID) {
            continue;
        }
        for item in &mut menu.items {
            if item.tag.as_deref() == Some(WINDOWS_MENU_LIST_ID) {
                item.submenu = items;
                return;
            }
        }
    }
}

fn main() -> Result<()> {
    let argv: Vec<String> = env::args().skip(1).collect();
    let shell_spec = TerminalSessionSpec::shell_from_env();
    let command_spec = argv.split_first().map(|(program, args)| {
        TerminalSessionSpec::command("Command", program.clone(), args.to_vec())
    });
    let initial_spec = command_spec.clone().unwrap_or_else(|| shell_spec.clone());
    let terminal_config_path = default_terminal_config_path();
    let terminal_config = Binding::new(load_terminal_config_or_default(
        terminal_config_path.as_deref(),
    )?);

    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    let (action_tx, action_rx) = mpsc::channel::<TerminalViewerAction>();
    let terminal_sessions: Rc<RefCell<Vec<TerminalWindowSession>>> =
        Rc::new(RefCell::new(Vec::new()));

    let initial_spec_for_build = initial_spec.clone();
    let action_tx_for_build = action_tx.clone();
    let terminal_sessions_for_build = Rc::clone(&terminal_sessions);

    let action_tx_for_actions = action_tx.clone();
    let action_tx_for_tick = action_tx.clone();
    let action_tx_for_event = action_tx.clone();
    let terminal_sessions_for_actions = Rc::clone(&terminal_sessions);
    let terminal_sessions_for_tick = Rc::clone(&terminal_sessions);
    let terminal_sessions_for_event = Rc::clone(&terminal_sessions);
    let command_context: Rc<RefCell<Option<CommandContextState>>> = Rc::new(RefCell::new(None));
    let command_context_for_actions = Rc::clone(&command_context);
    let command_context_for_event = Rc::clone(&command_context);
    let settings_window_id: Rc<RefCell<Option<WindowId>>> = Rc::new(RefCell::new(None));
    let settings_window_id_for_actions = Rc::clone(&settings_window_id);
    let terminal_config_for_actions = terminal_config.clone();
    let terminal_config_path_for_actions = terminal_config_path.clone();

    run_crossterm_desktop_with_actions(
        config,
        move |screen: Rect| {
            let theme = Theme::dark();
            let menu = build_menu(action_tx_for_build.clone());
            let mut desktop = Desktop::new(theme, menu);

            let session =
                spawn_terminal_window(&mut desktop, screen, 1, initial_spec_for_build.clone())?;
            terminal_sessions_for_build.borrow_mut().push(session);
            refresh_windows_menu(&mut desktop, &action_tx_for_build);

            Ok(desktop)
        },
        action_rx,
        {
            let shell_spec = shell_spec.clone();
            let command_spec = command_spec.clone();
            let action_tx = action_tx_for_actions.clone();
            let terminal_sessions = Rc::clone(&terminal_sessions_for_actions);
            let mut next_window_number = 2usize;

            move |desktop: &mut Desktop, action: TerminalViewerAction, screen: Rect| {
                match action {
                    TerminalViewerAction::NewShellWindow => {
                        let spec = {
                            let mut sessions = terminal_sessions.borrow_mut();
                            sync_session_cwds(&mut sessions);
                            spec_for_new_window(desktop, &sessions, &shell_spec)
                        };
                        let session =
                            spawn_terminal_window(desktop, screen, next_window_number, spec)?;
                        terminal_sessions.borrow_mut().push(session);
                        next_window_number = next_window_number.saturating_add(1);
                    }
                    TerminalViewerAction::NewCommandWindow => {
                        let base = command_spec.as_ref().unwrap_or(&shell_spec);
                        let spec = {
                            let mut sessions = terminal_sessions.borrow_mut();
                            sync_session_cwds(&mut sessions);
                            spec_for_new_window(desktop, &sessions, base)
                        };
                        let session =
                            spawn_terminal_window(desktop, screen, next_window_number, spec)?;
                        terminal_sessions.borrow_mut().push(session);
                        next_window_number = next_window_number.saturating_add(1);
                    }
                    TerminalViewerAction::OpenSettings => {
                        open_settings_window(
                            desktop,
                            screen,
                            terminal_config_for_actions.clone(),
                            terminal_config_path_for_actions.clone(),
                            &mut settings_window_id_for_actions.borrow_mut(),
                        );
                    }
                    TerminalViewerAction::Quit => return Ok(AppControl::Exit),
                    TerminalViewerAction::FocusNext => desktop.wm.focus_next(),
                    TerminalViewerAction::MinimizeFocused => desktop.wm.minimize_focused(),
                    TerminalViewerAction::ToggleMaximizeFocused => {
                        let work_area = Desktop::layout(screen).work_area;
                        desktop.wm.toggle_maximize_focused(work_area);
                    }
                    TerminalViewerAction::CloseFocused => {
                        if let Some(id) = desktop.wm.focused() {
                            desktop.wm.request_close(id);
                        }
                    }
                    TerminalViewerAction::FocusWindow(id) => {
                        if !desktop.wm.restore_window(id) {
                            desktop.wm.focus(id);
                        }
                    }
                    TerminalViewerAction::CommandContext(action) => {
                        let sessions = terminal_sessions.borrow();
                        apply_command_context_action(
                            desktop,
                            &sessions,
                            &mut command_context_for_actions.borrow_mut(),
                            action,
                        );
                    }
                }

                let mut sessions = terminal_sessions.borrow_mut();
                prune_terminal_sessions(desktop, &mut sessions);
                sync_session_cwds(&mut sessions);
                sync_terminal_window_titles(desktop, &sessions);
                refresh_windows_menu(desktop, &action_tx);
                Ok(AppControl::Continue)
            }
        },
        move |desktop: &mut Desktop, _screen: Rect| {
            let mut sessions = terminal_sessions_for_tick.borrow_mut();
            update_terminal_exit_prompts(desktop, &mut sessions);
            sync_session_cwds(&mut sessions);
            sync_terminal_window_titles(desktop, &sessions);
            refresh_windows_menu(desktop, &action_tx_for_tick);
            Ok(AppControl::Continue)
        },
        {
            move |desktop: &mut Desktop, ev, screen, _result| {
                if let Some(mouse) = is_right_mouse_down(ev) {
                    let sessions = terminal_sessions_for_event.borrow();
                    open_command_context_menu(
                        desktop,
                        &sessions,
                        screen,
                        mouse,
                        &action_tx_for_event,
                        &mut command_context_for_event.borrow_mut(),
                    );
                }
                if is_plain_restart_key(ev) {
                    let mut sessions = terminal_sessions_for_event.borrow_mut();
                    sync_session_cwds(&mut sessions);
                    let restarted = restart_focused_terminal(desktop, &mut sessions)?;
                    if restarted {
                        refresh_windows_menu(desktop, &action_tx_for_event);
                    }
                }
                Ok(AppControl::Continue)
            }
        },
    )
}
