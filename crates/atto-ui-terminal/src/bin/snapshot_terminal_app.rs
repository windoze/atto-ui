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

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_terminal::{TerminalEmulator, TerminalShortcut};

const SCROLL_LINES: usize = 40;
const RELEASE_SHORTCUT: TerminalShortcut = TerminalShortcut {
    code: KeyCode::Char('g'),
    modifiers: KeyModifiers::CONTROL,
};

/// RAII guard that puts the terminal into raw / alternate-screen / mouse-capture
/// mode and restores it on drop — including on `?`-propagated errors and panics,
/// which would otherwise leave the user's shell corrupted.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            event::EnableMouseCapture,
            cursor::Show
        )?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            event::DisableMouseCapture,
            cursor::Show
        );
        let _ = disable_raw_mode();
    }
}

fn describe_input(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        if (0x20..=0x7e).contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("<{b:02X}>"));
        }
    }
    out
}

fn main() -> Result<()> {
    // The guard restores the terminal on every exit path (early `?`, panic,
    // normal return), so the loop below can use `?` freely.
    let _guard = TerminalGuard::enter()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;
    let window_rect = Rect {
        x: work.x.saturating_add(2),
        y: work.y.saturating_add(2),
        width: 70.min(work.width.saturating_sub(2)).max(30),
        height: 18.min(work.height.saturating_sub(2)).max(10),
    };

    let terminal_view = TerminalEmulator::new()
        .scrollback_len(200)
        .release_shortcut(RELEASE_SHORTCUT);
    let handle = terminal_view.handle();

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Terminal",
            window_rect,
            Box::new(terminal_view),
        ),
        screen,
    );

    let mut last_capture = handle.capture();
    let mut seeded = false;

    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !seeded {
            handle.process_output_str("\x1b[?1000h\x1b[?1006h");
            for i in 0..SCROLL_LINES {
                handle.process_output_str(&format!("SCROLL-{i:02}\r\n"));
            }
            handle.process_output_str("READY\r\n");
            handle.process_output_str("\x1b[31mRED\x1b[m \x1b[32mGREEN\x1b[m");
            seeded = true;
        }

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        let screen: Rect = terminal.size()?.into();
        let _res = desktop.handle_event(&ev, screen);

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

        let input = handle.take_input();
        if !input.is_empty() {
            let text = describe_input(&input);
            if !text.is_empty() {
                handle.process_output_str(&format!("\r\nIN: {text}"));
            }
        }

        let capture_now = handle.capture();
        if capture_now != last_capture {
            let label = if capture_now { "ON" } else { "OFF" };
            handle.process_output_str(&format!("\r\n[CAPTURE {label}]"));
            last_capture = capture_now;
        }
    }

    // Terminal restoration is handled by `_guard`'s Drop.
    Ok(())
}
