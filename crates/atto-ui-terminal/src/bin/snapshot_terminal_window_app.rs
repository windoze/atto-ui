use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use atto_ui::app::{Desktop, DesktopAction, MenuBar, MenuItem, MenuSpec};
use atto_ui::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout, Scrollable, VStack,
};
use atto_ui::reactive::{Binding, DirtyObserver};
use atto_ui::theme::Theme;
use atto_ui::widgets::Label;
use atto_ui::wm::{Window, WindowId, WindowKind, WindowState};
use atto_ui_terminal::{
    TerminalCommandBlockPresentation, TerminalConfig, TerminalEmulator, TerminalHandle,
    TerminalPaneGroup, TerminalPaneGroupHandle, TerminalPaneId, TerminalSessionSpec,
    TerminalSettingsView, TerminalShortcutConfig, TerminalShortcutModifier,
    default_terminal_config_path, load_terminal_config_or_default,
};

const DEFAULT_TERMINAL_TITLE: &str = "Terminal";
const WINDOWS_MENU_ID: &str = "atto-ui-terminal:snapshot-window:windows";
const WINDOWS_MENU_LIST_ID: &str = "atto-ui-terminal:snapshot-window:windows:list";

struct SnapshotSessionConfig {
    initial: Option<TerminalSessionSpec>,
    shell: TerminalSessionSpec,
    command: Option<TerminalSessionSpec>,
    terminal_config_path: Option<PathBuf>,
}

fn terminal_session_config_from_env_args() -> SnapshotSessionConfig {
    let mut argv: Vec<String> = env::args().skip(1).collect();
    let mut cwd = None;
    let mut profile = "Command".to_string();
    let mut shell_program = None;
    let mut terminal_config_path = default_terminal_config_path();
    loop {
        match argv.first().map(String::as_str) {
            Some("--config") if argv.len() >= 2 => {
                terminal_config_path = Some(PathBuf::from(argv.remove(1)));
                argv.remove(0);
            }
            Some("--cwd") if argv.len() >= 2 => {
                cwd = Some(PathBuf::from(argv.remove(1)));
                argv.remove(0);
            }
            Some("--profile") if argv.len() >= 2 => {
                profile = argv.remove(1);
                argv.remove(0);
            }
            Some("--shell-program") if argv.len() >= 2 => {
                shell_program = Some(argv.remove(1));
                argv.remove(0);
            }
            _ => break,
        }
    }

    let mut shell = if let Some(program) = shell_program {
        TerminalSessionSpec::new("Shell", program, Vec::new())
    } else {
        TerminalSessionSpec::shell_from_env()
    };

    if let Some(cwd) = cwd.clone() {
        shell.set_cwd(cwd);
    }

    let command = if let Some((program, args)) = argv.split_first() {
        let mut spec = TerminalSessionSpec::command(profile, program.clone(), args.to_vec());
        if let Some(cwd) = cwd {
            spec.set_cwd(cwd);
        }
        Some(spec)
    } else {
        None
    };

    let initial = if let Some(spec) = command.clone() {
        Some(spec)
    } else if shell.cwd().is_some() {
        Some(shell.clone())
    } else {
        None
    };

    SnapshotSessionConfig {
        initial,
        shell,
        command,
        terminal_config_path,
    }
}

fn terminal_window_title(window_number: usize) -> String {
    if window_number == 1 {
        DEFAULT_TERMINAL_TITLE.to_string()
    } else {
        format!("Terminal {window_number}")
    }
}

fn terminal_window_rect(work: Rect, window_number: usize) -> Rect {
    let index = window_number.saturating_sub(1) as u16;
    Rect {
        x: work.x.saturating_add(2 + (index.saturating_mul(2) % 8)),
        y: work.y.saturating_add(2 + (index % 5)),
        width: 50.min(work.width.saturating_sub(2)).max(20),
        height: 14.min(work.height.saturating_sub(8)).max(8),
    }
}

fn load_snapshot_terminal_config(path: Option<&Path>) -> Result<TerminalConfig> {
    let mut config = load_terminal_config_or_default(path)?;
    if path.is_none_or(|path| !path.exists()) {
        config.release_shortcut =
            TerminalShortcutConfig::new("g", [TerminalShortcutModifier::Control]);
    }
    config.validate()?;
    Ok(config)
}

struct TerminalWindowSession {
    id: WindowId,
    window_number: usize,
    panes: TerminalPaneGroupHandle,
    spec: Option<TerminalSessionSpec>,
    exit_prompted: bool,
    restart_count: u32,
}

impl TerminalWindowSession {
    fn new(
        id: WindowId,
        window_number: usize,
        panes: TerminalPaneGroupHandle,
        spec: Option<TerminalSessionSpec>,
    ) -> Self {
        Self {
            id,
            window_number,
            panes,
            spec,
            exit_prompted: false,
            restart_count: 0,
        }
    }

    fn active_handle(&self) -> Option<TerminalHandle> {
        self.panes.active_terminal_handle()
    }
}

#[derive(Clone, Copy, Debug)]
enum CommandContextMenuAction {
    Rerun,
    CopyCommand,
    CopyOutput,
}

#[derive(Clone, Copy, Debug)]
struct CommandContextState {
    menu_id: WindowId,
    pane_id: TerminalPaneId,
    block_index: usize,
}

struct CommandContextMenuView {
    action_tx: mpsc::Sender<CommandContextMenuAction>,
    last_area: Option<Rect>,
}

impl CommandContextMenuView {
    fn new(action_tx: mpsc::Sender<CommandContextMenuAction>) -> Self {
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
        let _ = self.action_tx.send(action);
        EventResult::consumed()
    }
}

impl Layout for CommandContextMenuView {}
impl Scrollable for CommandContextMenuView {}
impl FocusNav for CommandContextMenuView {}
atto_ui::impl_component_default_traits!(CommandContextMenuView => DynamicTree);

fn build_status_view(lines: &[Binding<String>]) -> Box<dyn Component> {
    let mut stack = VStack::new();
    for line in lines {
        stack = stack.child(Label::new(line.clone()));
    }
    Box::new(stack)
}

fn build_menu(menu_action: &Binding<String>) -> MenuBar {
    let menu_action_cb = menu_action.clone();
    MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::action("Ping", {
                    let menu_action_cb = menu_action_cb.clone();
                    move || {
                        menu_action_cb.set("PING".to_string());
                    }
                }),
                MenuItem::action("New shell window", {
                    let menu_action_cb = menu_action_cb.clone();
                    move || {
                        menu_action_cb.set("NEW_SHELL".to_string());
                    }
                }),
                MenuItem::action("New command window", move || {
                    menu_action_cb.set("NEW_COMMAND".to_string());
                }),
                MenuItem::action("Settings", {
                    let menu_action_cb = menu_action.clone();
                    move || {
                        menu_action_cb.set("SETTINGS".to_string());
                    }
                }),
            ],
        ),
        MenuSpec::new(
            "Windows",
            vec![MenuItem::submenu("Switch to", Vec::new()).with_tag(WINDOWS_MENU_LIST_ID)],
        )
        .with_tag(WINDOWS_MENU_ID),
    ])
}

fn rect_text(rect: Option<Rect>) -> String {
    match rect {
        Some(r) => format!("{},{},{},{}", r.x, r.y, r.width, r.height),
        None => "CLOSED".to_string(),
    }
}

fn find_window_rect(desktop: &Desktop, id: WindowId) -> Option<Rect> {
    desktop
        .wm
        .windows()
        .iter()
        .find(|w| w.id() == id)
        .map(|w| w.rect.get())
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

fn build_terminal_view(
    spec: Option<&TerminalSessionSpec>,
    pane_number: usize,
    config: &TerminalConfig,
) -> Result<(TerminalEmulator, TerminalHandle)> {
    let mut terminal = TerminalEmulator::from_config(config)?
        .command_block_presentation(TerminalCommandBlockPresentation::enabled())
        .without_system_clipboard();
    if let Some(spec) = spec {
        terminal.spawn_session(spec)?;
    }
    let handle = terminal.handle();
    handle.process_output_str(&format!("TTY READY PANE={pane_number}\r\n"));
    Ok((terminal, handle))
}

fn build_terminal_pane_group(
    spec: Option<&TerminalSessionSpec>,
    config: Binding<TerminalConfig>,
) -> Result<(TerminalPaneGroup, TerminalPaneGroupHandle)> {
    let spec_owned = spec.cloned();
    let initial_config = config.get();
    let (terminal, _) = build_terminal_view(spec, 1, &initial_config)?;
    let config_for_factory = config.clone();
    let group = TerminalPaneGroup::new(terminal)
        .config(&initial_config)?
        .pane_factory(move |pane_number| {
            let config = config_for_factory.get();
            build_terminal_view(spec_owned.as_ref(), pane_number, &config)
                .map(|(terminal, _)| terminal)
        });
    let handle = group.handle();
    Ok((group, handle))
}

fn spawn_terminal_window(
    desktop: &mut Desktop,
    screen: Rect,
    window_number: usize,
    spec: Option<TerminalSessionSpec>,
    config: Binding<TerminalConfig>,
) -> Result<TerminalWindowSession> {
    let work = Desktop::layout(screen).work_area;
    let rect = terminal_window_rect(work, window_number);
    let (terminal, panes) = build_terminal_pane_group(spec.as_ref(), config)?;
    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            terminal_window_title(window_number),
            rect,
            Box::new(terminal),
        ),
        screen,
    );
    desktop.wm.focus(id);
    Ok(TerminalWindowSession::new(id, window_number, panes, spec))
}

fn settings_window_rect(screen: Rect) -> Rect {
    let work = Desktop::layout(screen).work_area;
    let width = 70.min(work.width.saturating_sub(4)).max(40);
    let height = 20.min(work.height.saturating_sub(2)).max(12);
    Rect {
        x: work.x + work.width.saturating_sub(width) / 2,
        y: work.y + work.height.saturating_sub(height) / 2,
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

fn show_exit_prompt_if_needed(desktop: &Desktop, session: &mut TerminalWindowSession) -> bool {
    if session.spec.is_none()
        || session.exit_prompted
        || find_window_rect(desktop, session.id).is_none()
    {
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

fn sync_session_cwd_from_active_pane(session: &mut TerminalWindowSession) -> bool {
    let Some(cwd) = session
        .active_handle()
        .and_then(|handle| handle.current_cwd())
    else {
        return false;
    };
    let Some(spec) = &mut session.spec else {
        return false;
    };
    let cwd = PathBuf::from(cwd);
    if spec.cwd() == Some(cwd.as_path()) {
        return false;
    }
    spec.set_cwd(cwd);
    true
}

fn sync_all_session_cwds(
    term_session: &mut TerminalWindowSession,
    extra_sessions: &mut [TerminalWindowSession],
) -> bool {
    let mut changed = sync_session_cwd_from_active_pane(term_session);
    for session in extra_sessions {
        changed |= sync_session_cwd_from_active_pane(session);
    }
    changed
}

fn focused_session_cwd(
    desktop: &Desktop,
    term_session: &TerminalWindowSession,
    extra_sessions: &[TerminalWindowSession],
) -> Option<PathBuf> {
    let focused = desktop.wm.focused()?;
    std::iter::once(term_session)
        .chain(extra_sessions.iter())
        .find(|session| session.id == focused)
        .and_then(|session| session.spec.as_ref())
        .and_then(|spec| spec.cwd().map(PathBuf::from))
}

fn spec_for_new_window(
    desktop: &Desktop,
    term_session: &TerminalWindowSession,
    extra_sessions: &[TerminalWindowSession],
    base: &TerminalSessionSpec,
) -> TerminalSessionSpec {
    let mut spec = base.clone();
    if let Some(cwd) = focused_session_cwd(desktop, term_session, extra_sessions) {
        spec.set_cwd(cwd);
    }
    spec
}

fn sync_terminal_window_title(desktop: &mut Desktop, session: &TerminalWindowSession) -> bool {
    let Some(handle) = session.active_handle() else {
        return false;
    };
    let Some(title) = handle.window_title() else {
        return false;
    };
    let current_title = desktop.wm.window(session.id).map(|w| w.title.get());
    if current_title.as_deref() == Some(title.as_str()) {
        return false;
    }
    desktop.set_title(session.id, title)
}

fn refresh_windows_menu(desktop: &mut Desktop) {
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
            let id = w.id();
            (w.title.get(), id, Some(id) == focused)
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
        items.push(MenuItem::action(label, move || {
            let _ = id;
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

fn restart_terminal_view(
    desktop: &mut Desktop,
    session: &mut TerminalWindowSession,
    config: Binding<TerminalConfig>,
) -> Result<bool> {
    if !session.exit_prompted || desktop.wm.focused() != Some(session.id) {
        return Ok(false);
    }
    let Some(spec) = session.spec.clone() else {
        return Ok(false);
    };

    let (terminal, panes) = build_terminal_pane_group(Some(&spec), config)?;
    if !desktop.wm.set_view(session.id, Box::new(terminal)) {
        return Ok(false);
    }
    session.panes = panes;
    session.exit_prompted = false;
    session.restart_count = session.restart_count.saturating_add(1);
    desktop.set_title(session.id, terminal_window_title(session.window_number));
    desktop.wm.focus(session.id);
    Ok(true)
}

fn apply_terminal_config_to_sessions(
    config: &TerminalConfig,
    term_session: &TerminalWindowSession,
    extra_sessions: &[TerminalWindowSession],
) -> Result<()> {
    term_session.panes.apply_config(config)?;
    for session in extra_sessions {
        session.panes.apply_config(config)?;
    }
    Ok(())
}

fn apply_terminal_config_if_dirty(
    config: &Binding<TerminalConfig>,
    observer: &mut DirtyObserver,
    term_session: &TerminalWindowSession,
    extra_sessions: &[TerminalWindowSession],
) -> Result<()> {
    if config.check_dirty(observer) {
        apply_terminal_config_to_sessions(&config.get(), term_session, extra_sessions)?;
    }
    Ok(())
}

fn close_command_context_menu(desktop: &mut Desktop, context: &mut Option<CommandContextState>) {
    if let Some(context) = context.take() {
        desktop.wm.close(context.menu_id);
    }
}

fn command_block_at_mouse(
    desktop: &Desktop,
    session: &TerminalWindowSession,
    mouse: &MouseEvent,
) -> Option<(TerminalPaneId, usize)> {
    let inner = desktop.wm.window(session.id)?.inner_rect();
    if mouse.column < inner.x
        || mouse.row < inner.y
        || mouse.column >= inner.x.saturating_add(inner.width)
        || mouse.row >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let pane = session
        .panes
        .pane_at_screen_position(mouse.column, mouse.row)?;
    let rect = pane.rect?;
    let row = mouse.row.saturating_sub(rect.y);
    let col = mouse.column.saturating_sub(rect.x);
    let position = pane.handle.selection_position_for_view_cell(row, col);
    pane.handle
        .command_block_index_at_position(position)
        .map(|block_index| (pane.id, block_index))
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
    session: &TerminalWindowSession,
    screen: Rect,
    mouse: &MouseEvent,
    action_tx: &mpsc::Sender<CommandContextMenuAction>,
    context: &mut Option<CommandContextState>,
) -> bool {
    close_command_context_menu(desktop, context);
    let Some((pane_id, block_index)) = command_block_at_mouse(desktop, session, mouse) else {
        return false;
    };
    if let Some(pane) = session
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
        pane_id,
        block_index,
    });
    true
}

fn apply_command_context_action(
    desktop: &mut Desktop,
    session: &TerminalWindowSession,
    context: &mut Option<CommandContextState>,
    action: CommandContextMenuAction,
) {
    let Some(active) = *context else {
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

fn is_non_right_mouse_down(event: &Event) -> bool {
    matches!(
        event,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left | MouseButton::Middle),
            ..
        })
    )
}

fn process_status_text(session: &TerminalWindowSession) -> String {
    let handle = session.active_handle();
    let state = if session.spec.is_none() {
        "NONE".to_string()
    } else if handle.as_ref().is_some_and(TerminalHandle::is_running) {
        "RUNNING".to_string()
    } else if let Some(status) = handle.as_ref().and_then(TerminalHandle::exit_status) {
        format!("EXITED code={}", status.exit_code())
    } else {
        "STOPPED".to_string()
    };
    format!("PROC={state} RESTARTS={}", session.restart_count)
}

fn session_status_text(session: &TerminalWindowSession) -> String {
    let Some(spec) = &session.spec else {
        return "SESSION=NONE CWD=-".to_string();
    };
    let cwd = spec.cwd().and_then(|cwd| cwd.to_str()).unwrap_or("-");
    format!("SESSION={} CWD={cwd}", spec.profile())
}

fn copy_status_text(session: &TerminalWindowSession) -> String {
    let Some(handle) = session.active_handle() else {
        return "COPYMODE=OFF COPY= SEL=".to_string();
    };
    let mode = if handle.copy_mode() { "ON" } else { "OFF" };
    let copied = handle
        .copied_text()
        .unwrap_or_default()
        .replace('\n', "\\n");
    let selected = handle
        .selected_text()
        .unwrap_or_default()
        .replace('\n', "\\n");
    format!("COPYMODE={mode} COPY={copied} SEL={selected}")
}

fn pane_status_text(session: &TerminalWindowSession) -> String {
    let panes = session.panes.panes();
    let active = session
        .panes
        .active_pane_id()
        .map(|id| id.raw().to_string())
        .unwrap_or_else(|| "NONE".to_string());
    let rects = panes
        .iter()
        .map(|pane| {
            let rect = rect_text(pane.rect);
            format!("{}:{rect}", pane.id.raw())
        })
        .collect::<Vec<_>>()
        .join(";");
    format!("PANES={} ACTIVE={} PANE_RECTS={rects}", panes.len(), active)
}

fn window_status_text(
    desktop: &Desktop,
    term_session: &TerminalWindowSession,
    extra_sessions: &[TerminalWindowSession],
) -> String {
    let sessions = std::iter::once(term_session).chain(extra_sessions.iter());
    let mut terminal_count = 0usize;
    let mut focused_session = None;
    let focused = desktop.wm.focused();
    for session in sessions {
        if find_window_rect(desktop, session.id).is_none() {
            continue;
        }
        terminal_count = terminal_count.saturating_add(1);
        if Some(session.id) == focused {
            focused_session = Some(session);
        }
    }

    let Some(session) = focused_session else {
        return format!("TERMS={terminal_count} FOCUS_TERM=NONE FOCUS_PROFILE=- FOCUS_CWD=-");
    };
    let profile = session
        .spec
        .as_ref()
        .map(|spec| spec.profile())
        .unwrap_or("NONE");
    let cwd = session
        .spec
        .as_ref()
        .and_then(|spec| spec.cwd())
        .and_then(|cwd| cwd.to_str())
        .unwrap_or("-");
    format!(
        "TERMS={terminal_count} FOCUS_TERM={} FOCUS_PROFILE={profile} FOCUS_CWD={cwd}",
        session.window_number
    )
}

#[allow(clippy::too_many_arguments)]
fn update_status_lines(
    desktop: &Desktop,
    term_session: &TerminalWindowSession,
    extra_sessions: &[TerminalWindowSession],
    tools_id: WindowId,
    menu_action: &Binding<String>,
    focus_line: &Binding<String>,
    rect_line: &Binding<String>,
    pane_line: &Binding<String>,
    process_line: &Binding<String>,
) {
    let term_id = term_session.id;
    let focused = match desktop.wm.focused() {
        Some(id) if id == term_id => "TERM",
        Some(id) if id == tools_id => "TOOLS",
        Some(_) => "OTHER",
        None => "NONE",
    };
    let capture = if term_session
        .active_handle()
        .as_ref()
        .is_some_and(TerminalHandle::capture)
    {
        "ON"
    } else {
        "OFF"
    };
    focus_line.set(format!(
        "FOCUS={focused} CAP={capture} {}",
        window_status_text(desktop, term_session, extra_sessions)
    ));

    let menu_state = if desktop.menu.is_active() {
        "ON"
    } else {
        "OFF"
    };
    let term_rect = rect_text(find_window_rect(desktop, term_id));
    let tools_rect = rect_text(find_window_rect(desktop, tools_id));
    rect_line.set(format!(
        "RECT TERM={term_rect} TOOLS={tools_rect} MENU={} ACTIVE={menu_state}",
        menu_action.get()
    ));
    pane_line.set(pane_status_text(term_session));
    process_line.set(format!(
        "{} {} {}",
        process_status_text(term_session),
        copy_status_text(term_session),
        session_status_text(term_session)
    ));
}

fn main() -> Result<()> {
    let session_config = terminal_session_config_from_env_args();
    let terminal_config_path = session_config.terminal_config_path.clone();
    let terminal_config = Binding::new(load_snapshot_terminal_config(
        terminal_config_path.as_deref(),
    )?);
    let mut terminal_config_observer = terminal_config.dirty_observer();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        cursor::Show
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let menu_action = Binding::new("NONE".to_string());
    let menu = build_menu(&menu_action);
    let (command_menu_tx, command_menu_rx) = mpsc::channel();
    let mut command_context: Option<CommandContextState> = None;
    let mut settings_window_id: Option<WindowId> = None;

    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    let focus_line = Binding::new(String::new());
    let rect_line = Binding::new(String::new());
    let pane_line = Binding::new(String::new());
    let process_line = Binding::new(String::new());

    let status_view = build_status_view(&[
        focus_line.clone(),
        rect_line.clone(),
        pane_line.clone(),
        process_line.clone(),
    ]);

    let tools_rect = Rect {
        x: work.x.saturating_add(55),
        y: work.y.saturating_add(3),
        width: 22.min(work.width.saturating_sub(55)).max(14),
        height: 10.min(work.height.saturating_sub(6)).max(7),
    };
    let status_rect = Rect {
        x: work.x.saturating_add(2),
        y: work.y.saturating_add(work.height.saturating_sub(6)),
        width: work.width.saturating_sub(4).max(10),
        height: 6.min(work.height.saturating_sub(1)).max(4),
    };

    let mut term_session = spawn_terminal_window(
        &mut desktop,
        screen,
        1,
        session_config.initial,
        terminal_config.clone(),
    )?;
    let term_id = term_session.id;
    let mut extra_sessions = Vec::new();
    let mut next_window_number = 2usize;
    let tools_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Tools",
            tools_rect,
            Box::new(Label::new("Tools")),
        ),
        screen,
    );
    let _status_id = desktop.add_window(
        Window::new(WindowKind::Tooltip, "Status", status_rect, status_view),
        screen,
    );
    desktop.wm.focus(term_id);
    refresh_windows_menu(&mut desktop);

    loop {
        show_exit_prompt_if_needed(&desktop, &mut term_session);
        for session in &mut extra_sessions {
            show_exit_prompt_if_needed(&desktop, session);
        }
        sync_all_session_cwds(&mut term_session, &mut extra_sessions);
        sync_terminal_window_title(&mut desktop, &term_session);
        for session in &extra_sessions {
            sync_terminal_window_title(&mut desktop, session);
        }
        apply_terminal_config_if_dirty(
            &terminal_config,
            &mut terminal_config_observer,
            &term_session,
            &extra_sessions,
        )?;
        refresh_windows_menu(&mut desktop);
        update_status_lines(
            &desktop,
            &term_session,
            &extra_sessions,
            tools_id,
            &menu_action,
            &focus_line,
            &rect_line,
            &pane_line,
            &process_line,
        );

        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        let screen: Rect = terminal.size()?.into();
        let res = desktop.handle_event(&ev, screen);
        if let DesktopAction::CloseWindow(id) = res.action {
            desktop.wm.close(id);
            if settings_window_id == Some(id) {
                settings_window_id = None;
            }
        }
        if let Some(mouse) = is_right_mouse_down(&ev) {
            open_command_context_menu(
                &mut desktop,
                &term_session,
                screen,
                mouse,
                &command_menu_tx,
                &mut command_context,
            );
        }
        let mut handled_context_action = false;
        while let Ok(action) = command_menu_rx.try_recv() {
            handled_context_action = true;
            apply_command_context_action(&mut desktop, &term_session, &mut command_context, action);
        }
        if !handled_context_action && is_non_right_mouse_down(&ev) {
            close_command_context_menu(&mut desktop, &mut command_context);
        }
        match menu_action.get().as_str() {
            "NEW_SHELL" => {
                sync_all_session_cwds(&mut term_session, &mut extra_sessions);
                let spec = spec_for_new_window(
                    &desktop,
                    &term_session,
                    &extra_sessions,
                    &session_config.shell,
                );
                let session = spawn_terminal_window(
                    &mut desktop,
                    screen,
                    next_window_number,
                    Some(spec),
                    terminal_config.clone(),
                )?;
                extra_sessions.push(session);
                next_window_number = next_window_number.saturating_add(1);
                menu_action.set("SHELL".to_string());
            }
            "NEW_COMMAND" => {
                sync_all_session_cwds(&mut term_session, &mut extra_sessions);
                let base = session_config
                    .command
                    .as_ref()
                    .unwrap_or(&session_config.shell);
                let spec = spec_for_new_window(&desktop, &term_session, &extra_sessions, base);
                let session = spawn_terminal_window(
                    &mut desktop,
                    screen,
                    next_window_number,
                    Some(spec),
                    terminal_config.clone(),
                )?;
                extra_sessions.push(session);
                next_window_number = next_window_number.saturating_add(1);
                menu_action.set("COMMAND".to_string());
            }
            "SETTINGS" => {
                open_settings_window(
                    &mut desktop,
                    screen,
                    terminal_config.clone(),
                    terminal_config_path.clone(),
                    &mut settings_window_id,
                );
                menu_action.set("SETTINGS_OPEN".to_string());
            }
            _ => {}
        }
        if is_plain_restart_key(&ev) {
            sync_all_session_cwds(&mut term_session, &mut extra_sessions);
            if !restart_terminal_view(&mut desktop, &mut term_session, terminal_config.clone())? {
                for session in &mut extra_sessions {
                    if restart_terminal_view(&mut desktop, session, terminal_config.clone())? {
                        break;
                    }
                }
            }
        }

        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = ev
            && modifiers.contains(KeyModifiers::CONTROL)
        {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
