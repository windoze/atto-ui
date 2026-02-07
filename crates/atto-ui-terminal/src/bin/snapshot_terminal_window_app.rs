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
        .find(|w| w.id == id)
        .map(|w| w.rect.get())
}

fn update_status_lines(
    desktop: &Desktop,
    term_id: WindowId,
    tools_id: WindowId,
    term_handle: &TerminalHandle,
    menu_action: &Binding<String>,
    focus_line: &Binding<String>,
    rect_line: &Binding<String>,
    menu_line: &Binding<String>,
) {
    let focused = match desktop.wm.focused() {
        Some(id) if id == term_id => "TERM",
        Some(id) if id == tools_id => "TOOLS",
        Some(_) => "OTHER",
        None => "NONE",
    };
    let capture = if term_handle.capture() { "ON" } else { "OFF" };
    focus_line.set(format!("FOCUS={focused} CAP={capture}"));

    let term_rect = rect_text(find_window_rect(desktop, term_id));
    let tools_rect = rect_text(find_window_rect(desktop, tools_id));
    rect_line.set(format!("RECT TERM={term_rect} TOOLS={tools_rect}"));

    let menu_state = if desktop.menu.is_active() { "ON" } else { "OFF" };
    menu_line.set(format!("MENU={} ACTIVE={menu_state}", menu_action.get()));
}

fn main() -> Result<()> {
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

    let status_view = build_status_view(&[focus_line.clone(), rect_line.clone(), menu_line.clone()]);

    let term_view = TerminalEmulator::new();
    let term_handle = term_view.handle();

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
        y: work.y.saturating_add(work.height.saturating_sub(5)),
        width: work.width.saturating_sub(4).max(10),
        height: 5.min(work.height.saturating_sub(1)).max(3),
    };

    let term_id = desktop.add_window(
        Window::new(WindowKind::Normal, "Terminal", term_rect, Box::new(term_view)),
        screen,
    );
    let tools_id = desktop.add_window(
        Window::new(WindowKind::Normal, "Tools", tools_rect, Box::new(Label::new("Tools"))),
        screen,
    );
    let _status_id = desktop.add_window(
        Window::new(WindowKind::Tooltip, "Status", status_rect, status_view),
        screen,
    );
    desktop.wm.focus(term_id);

    let mut seeded = false;

    loop {
        update_status_lines(
            &desktop,
            term_id,
            tools_id,
            &term_handle,
            &menu_action,
            &focus_line,
            &rect_line,
            &menu_line,
        );

        terminal.draw(|f| desktop.draw(f))?;

        if !seeded {
            term_handle.process_output_str("TTY READY\r\n");
            seeded = true;
        }

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        let screen: Rect = terminal.size()?.into();
        let res = desktop.handle_event(&ev, screen);
        if let DesktopAction::CloseWindow(id) = res.action {
            desktop.wm.close(id);
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
