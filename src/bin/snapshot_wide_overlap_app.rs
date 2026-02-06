use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Frame, Terminal};

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::composable::{Component, ComponentContext, EventResult};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};

/// Background window content that draws two wide glyphs:
/// - `好` is fully visible (sanity check)
/// - `你` is placed so its right half is overlapped by the foreground window border
struct BgView;

impl Component for BgView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let style = if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };

        let buf = frame.buffer_mut();
        buf.set_string(area.x, area.y, "Wide overlap demo", style);

        // Coordinates are chosen to line up with the hardcoded window rectangles in `main`.
        //
        // With BG window rect `{ x: 2, y: 2, ... }`, BG inner area starts at `{ x: 3, y: 3 }`.
        // This means:
        // - `好` lands at screen cell (4, 4) and should remain visible.
        // - `你` lands at screen cell (8, 4) with its second cell at (9, 4), which is overlapped
        //   by the FG window's left border at x=9.
        buf.set_string(area.x + 1, area.y + 1, "好", style);
        buf.set_string(area.x + 5, area.y + 1, "你", style);
    }
}

/// Foreground window content isn't important; the border is what we need.
struct FgView;

impl Component for FgView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let style = ctx.theme.widget.normal;
        frame.buffer_mut().set_string(area.x, area.y, "FG", style);
    }
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

    let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    // Keep this deterministic for PTY tests. These rectangles are expected to fit inside
    // the standard test screen (80x24).
    let bg_rect = Rect {
        x: work.x.saturating_add(2),
        y: work.y.saturating_add(1),
        width: 18,
        height: 6,
    };
    // Left border at x=9 (overlaps right-half of `你` at x=8).
    let fg_rect = Rect {
        x: bg_rect.x.saturating_add(7),
        y: bg_rect.y.saturating_add(1),
        width: 12,
        height: 5,
    };

    let _bg_id = desktop.add_window(
        Window::new(WindowKind::Normal, "BG", bg_rect, Box::new(BgView)),
        screen,
    );
    // Use a Tooltip so it stays on top without stealing focus (keeps its border "inactive").
    let _fg_id = desktop.add_window(
        Window::new(WindowKind::Tooltip, "FG", fg_rect, Box::new(FgView)),
        screen,
    );

    let res = run(&mut terminal, &mut desktop);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, desktop: &mut Desktop) -> Result<()> {
    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        // Deterministic quit key for PTY tests: accept both Ctrl+Q and raw DC1 (0x11).
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
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('\u{11}'),
            kind: KeyEventKind::Press,
            ..
        }) = ev
        {
            break;
        }

        let screen: Rect = terminal.size()?.into();
        let _ = desktop.handle_event(&ev, screen);
    }

    Ok(())
}
