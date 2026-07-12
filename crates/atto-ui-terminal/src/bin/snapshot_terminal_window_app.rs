use std::env;
use std::io;
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
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::widgets::Label;
use atto_ui::wm::{Window, WindowId, WindowKind, WindowState};
use atto_ui_terminal::{
    TerminalCommandBlockPresentation, TerminalEmulator, TerminalHandle, TerminalPaneGroup,
    TerminalPaneGroupHandle, TerminalPaneId, TerminalShortcut,
};

const DEFAULT_TERMINAL_TITLE: &str = "Terminal";
const WINDOWS_MENU_ID: &str = "atto-ui-terminal:snapshot-window:windows";
const WINDOWS_MENU_LIST_ID: &str = "atto-ui-terminal:snapshot-window:windows:list";

#[derive(Clone)]
struct TerminalCommand {
    program: String,
    args: Vec<String>,
}

impl TerminalCommand {
    fn from_env_args() -> Option<Self> {
        let mut args = env::args().skip(1);
        let program = args.next()?;
        Some(Self {
            program,
            args: args.collect(),
        })
    }
}

struct TerminalWindowSession {
    id: WindowId,
    panes: TerminalPaneGroupHandle,
    command: Option<TerminalCommand>,
    exit_prompted: bool,
    restart_count: u32,
}

impl TerminalWindowSession {
    fn new(id: WindowId, panes: TerminalPaneGroupHandle, command: Option<TerminalCommand>) -> Self {
        Self {
            id,
            panes,
            command,
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
            vec![MenuItem::action("Ping", move || {
                menu_action_cb.set("PING".to_string());
            })],
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
    command: Option<&TerminalCommand>,
    pane_number: usize,
) -> Result<(TerminalEmulator, TerminalHandle)> {
    let mut terminal = TerminalEmulator::new()
        .release_shortcut(TerminalShortcut::new(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
        ))
        .command_block_presentation(TerminalCommandBlockPresentation::enabled())
        .without_system_clipboard();
    if let Some(command) = command {
        terminal.spawn_process(&command.program, &command.args)?;
    }
    let handle = terminal.handle();
    handle.process_output_str(&format!("TTY READY PANE={pane_number}\r\n"));
    Ok((terminal, handle))
}

fn build_terminal_pane_group(
    command: Option<&TerminalCommand>,
) -> Result<(TerminalPaneGroup, TerminalPaneGroupHandle)> {
    let command_owned = command.cloned();
    let (terminal, _) = build_terminal_view(command, 1)?;
    let group = TerminalPaneGroup::new(terminal).pane_factory(move |pane_number| {
        build_terminal_view(command_owned.as_ref(), pane_number).map(|(terminal, _)| terminal)
    });
    let handle = group.handle();
    Ok((group, handle))
}

fn show_exit_prompt_if_needed(desktop: &Desktop, session: &mut TerminalWindowSession) -> bool {
    if session.command.is_none()
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
) -> Result<bool> {
    if !session.exit_prompted || desktop.wm.focused() != Some(session.id) {
        return Ok(false);
    }
    let Some(command) = session.command.clone() else {
        return Ok(false);
    };

    let (terminal, panes) = build_terminal_pane_group(Some(&command))?;
    if !desktop.wm.set_view(session.id, Box::new(terminal)) {
        return Ok(false);
    }
    session.panes = panes;
    session.exit_prompted = false;
    session.restart_count = session.restart_count.saturating_add(1);
    desktop.set_title(session.id, DEFAULT_TERMINAL_TITLE);
    desktop.wm.focus(session.id);
    Ok(true)
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
    let state = if session.command.is_none() {
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

#[allow(clippy::too_many_arguments)]
fn update_status_lines(
    desktop: &Desktop,
    term_session: &TerminalWindowSession,
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
    focus_line.set(format!("FOCUS={focused} CAP={capture}"));

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
        "{} {}",
        process_status_text(term_session),
        copy_status_text(term_session)
    ));
}

fn main() -> Result<()> {
    let terminal_command = TerminalCommand::from_env_args();

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

    let (term_view, term_panes) = build_terminal_pane_group(terminal_command.as_ref())?;

    let term_rect = Rect {
        x: work.x.saturating_add(2),
        y: work.y.saturating_add(2),
        width: 50.min(work.width.saturating_sub(2)).max(20),
        height: 14.min(work.height.saturating_sub(8)).max(8),
    };
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

    let term_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            DEFAULT_TERMINAL_TITLE,
            term_rect,
            Box::new(term_view),
        ),
        screen,
    );
    let mut term_session =
        TerminalWindowSession::new(term_id, term_panes, terminal_command.clone());
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
        sync_terminal_window_title(&mut desktop, &term_session);
        refresh_windows_menu(&mut desktop);
        update_status_lines(
            &desktop,
            &term_session,
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
        if is_plain_restart_key(&ev) {
            restart_terminal_view(&mut desktop, &mut term_session)?;
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
