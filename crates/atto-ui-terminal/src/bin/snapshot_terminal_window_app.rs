use std::env;
use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

use atto_ui::app::{Desktop, DesktopAction, MenuBar, MenuItem, MenuSpec};
use atto_ui::composable::{Component, VStack};
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::widgets::Label;
use atto_ui::wm::{Window, WindowId, WindowKind};
use atto_ui_terminal::{TerminalEmulator, TerminalHandle};

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
    handle: TerminalHandle,
    command: Option<TerminalCommand>,
    exit_prompted: bool,
    restart_count: u32,
}

impl TerminalWindowSession {
    fn new(id: WindowId, handle: TerminalHandle, command: Option<TerminalCommand>) -> Self {
        Self {
            id,
            handle,
            command,
            exit_prompted: false,
            restart_count: 0,
        }
    }
}

fn build_status_view(lines: &[Binding<String>]) -> Box<dyn Component> {
    let mut stack = VStack::new();
    for line in lines {
        stack = stack.child(Label::new(line.clone()));
    }
    Box::new(stack)
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
) -> Result<(TerminalEmulator, TerminalHandle)> {
    let mut terminal = TerminalEmulator::new();
    if let Some(command) = command {
        terminal.spawn_process(&command.program, &command.args)?;
    }
    let handle = terminal.handle();
    handle.process_output_str("TTY READY\r\n");
    Ok((terminal, handle))
}

fn show_exit_prompt_if_needed(desktop: &Desktop, session: &mut TerminalWindowSession) -> bool {
    if session.command.is_none()
        || session.exit_prompted
        || find_window_rect(desktop, session.id).is_none()
    {
        return false;
    }
    let Some(status) = session.handle.exit_status() else {
        return false;
    };

    session.handle.set_capture(false);
    session.handle.process_output_str(&format!(
        "\r\n[Process exited: code {} — press R to restart]\r\n",
        status.exit_code()
    ));
    session.exit_prompted = true;
    true
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

    let (terminal, handle) = build_terminal_view(Some(&command))?;
    if !desktop.wm.set_view(session.id, Box::new(terminal)) {
        return Ok(false);
    }
    session.handle = handle;
    session.exit_prompted = false;
    session.restart_count = session.restart_count.saturating_add(1);
    desktop.wm.focus(session.id);
    Ok(true)
}

fn process_status_text(session: &TerminalWindowSession) -> String {
    let state = if session.command.is_none() {
        "NONE".to_string()
    } else if session.handle.is_running() {
        "RUNNING".to_string()
    } else if let Some(status) = session.handle.exit_status() {
        format!("EXITED code={}", status.exit_code())
    } else {
        "STOPPED".to_string()
    };
    format!("PROC={state} RESTARTS={}", session.restart_count)
}

#[allow(clippy::too_many_arguments)]
fn update_status_lines(
    desktop: &Desktop,
    term_session: &TerminalWindowSession,
    tools_id: WindowId,
    menu_action: &Binding<String>,
    focus_line: &Binding<String>,
    rect_line: &Binding<String>,
    menu_line: &Binding<String>,
    process_line: &Binding<String>,
) {
    let term_id = term_session.id;
    let focused = match desktop.wm.focused() {
        Some(id) if id == term_id => "TERM",
        Some(id) if id == tools_id => "TOOLS",
        Some(_) => "OTHER",
        None => "NONE",
    };
    let capture = if term_session.handle.capture() {
        "ON"
    } else {
        "OFF"
    };
    focus_line.set(format!("FOCUS={focused} CAP={capture}"));

    let term_rect = rect_text(find_window_rect(desktop, term_id));
    let tools_rect = rect_text(find_window_rect(desktop, tools_id));
    rect_line.set(format!("RECT TERM={term_rect} TOOLS={tools_rect}"));

    let menu_state = if desktop.menu.is_active() {
        "ON"
    } else {
        "OFF"
    };
    menu_line.set(format!("MENU={} ACTIVE={menu_state}", menu_action.get()));
    process_line.set(process_status_text(term_session));
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
    let menu_action_cb = menu_action.clone();
    let menu = MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![MenuItem::action("Ping", move || {
            menu_action_cb.set("PING".to_string());
        })],
    )]);

    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    let focus_line = Binding::new(String::new());
    let rect_line = Binding::new(String::new());
    let menu_line = Binding::new(String::new());
    let process_line = Binding::new(String::new());

    let status_view = build_status_view(&[
        focus_line.clone(),
        rect_line.clone(),
        menu_line.clone(),
        process_line.clone(),
    ]);

    let (term_view, term_handle) = build_terminal_view(terminal_command.as_ref())?;

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
            "Terminal",
            term_rect,
            Box::new(term_view),
        ),
        screen,
    );
    let mut term_session =
        TerminalWindowSession::new(term_id, term_handle, terminal_command.clone());
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

    loop {
        show_exit_prompt_if_needed(&desktop, &mut term_session);
        update_status_lines(
            &desktop,
            &term_session,
            tools_id,
            &menu_action,
            &focus_line,
            &rect_line,
            &menu_line,
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
