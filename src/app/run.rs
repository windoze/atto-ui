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

use crate::app::{Desktop, DesktopAction, DesktopEventResult};
use crate::view::EventOutcome;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorMode {
    Show,
    #[default]
    Hide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrosstermAppConfig {
    pub tick_rate: Duration,
    pub enable_mouse_capture: bool,
    pub enable_bracketed_paste: bool,
    pub cursor: CursorMode,
}

impl Default for CrosstermAppConfig {
    fn default() -> Self {
        Self {
            tick_rate: Duration::from_millis(16),
            enable_mouse_capture: true,
            enable_bracketed_paste: false,
            cursor: CursorMode::Hide,
        }
    }
}

impl CrosstermAppConfig {
    pub fn tick_rate(self, tick_rate: Duration) -> Self {
        Self { tick_rate, ..self }
    }

    pub fn mouse_capture(self, enable_mouse_capture: bool) -> Self {
        Self {
            enable_mouse_capture,
            ..self
        }
    }

    pub fn bracketed_paste(self, enable_bracketed_paste: bool) -> Self {
        Self {
            enable_bracketed_paste,
            ..self
        }
    }

    pub fn cursor(self, cursor: CursorMode) -> Self {
        Self { cursor, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppControl {
    Continue,
    Exit,
}

pub fn should_quit_default(event: &Event, outcome: EventOutcome) -> bool {
    match event {
        // Ctrl+Q always quits.
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => true,
        // 'q' quits only when the event was not consumed by the UI.
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            ..
        }) => outcome == EventOutcome::Ignored,
        _ => false,
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn new(config: CrosstermAppConfig) -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();

        execute!(stdout, EnterAlternateScreen)?;
        if config.enable_mouse_capture {
            execute!(stdout, event::EnableMouseCapture)?;
        }
        if config.enable_bracketed_paste {
            execute!(stdout, event::EnableBracketedPaste)?;
        }
        match config.cursor {
            CursorMode::Show => execute!(stdout, cursor::Show)?,
            CursorMode::Hide => execute!(stdout, cursor::Hide)?,
        }

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        Ok(Self { terminal })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();

        let backend = self.terminal.backend_mut();
        let _ = execute!(backend, LeaveAlternateScreen);
        let _ = execute!(backend, event::DisableMouseCapture);
        let _ = execute!(backend, event::DisableBracketedPaste);
        let _ = execute!(backend, cursor::Show);

        let _ = self.terminal.show_cursor();
    }
}

fn handle_desktop_action(desktop: &mut Desktop, action: &DesktopAction) {
    match *action {
        DesktopAction::None => {}
        DesktopAction::CloseWindow(id) => {
            desktop.wm.close(id);
        }
    }
}

pub fn run_crossterm_desktop_simple<B>(config: CrosstermAppConfig, build: B) -> Result<()>
where
    B: FnOnce(Rect) -> Result<Desktop>,
{
    run_crossterm_desktop(
        config,
        build,
        |_desktop, _screen| Ok(AppControl::Continue),
        |_, _, _, _| Ok(AppControl::Continue),
    )
}

pub fn run_crossterm_desktop<B, TTick, TEvent>(
    config: CrosstermAppConfig,
    build: B,
    mut on_tick: TTick,
    mut on_event: TEvent,
) -> Result<()>
where
    B: FnOnce(Rect) -> Result<Desktop>,
    TTick: FnMut(&mut Desktop, Rect) -> Result<AppControl>,
    TEvent: FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>,
{
    let mut session = TerminalSession::new(config)?;
    let screen: Rect = session.terminal.size()?.into();
    let mut desktop = build(screen)?;

    loop {
        let screen: Rect = session.terminal.size()?.into();

        if on_tick(&mut desktop, screen)? == AppControl::Exit {
            break;
        }

        session.terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(config.tick_rate)? {
            continue;
        }

        let ev = event::read()?;
        let screen: Rect = session.terminal.size()?.into();
        let result = desktop.handle_event(&ev, screen);
        handle_desktop_action(&mut desktop, &result.action);

        if should_quit_default(&ev, result.outcome) {
            break;
        }
        if on_event(&mut desktop, &ev, screen, &result)? == AppControl::Exit {
            break;
        }
    }

    Ok(())
}
