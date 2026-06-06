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

use atto_ui::app::{Desktop, MenuBar, MenuItem, MenuSpec};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::{Disclosure, DisclosureStatus};
use atto_ui::wm::{Window, WindowKind};

fn append_chunk(content: &Property<String>) {
    content.update(|text| {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("chunk 2 appended");
    });
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

    let actions: EventQueue<()> = EventQueue::new();
    let menu = MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![
            MenuItem::action("Quit", {
                let actions = actions.clone();
                move || actions.push(())
            })
            .shortcut("q"),
        ],
    )]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let content = Property::new("chunk 1 ready".to_string());
    let status = Property::new(DisclosureStatus::Running);
    let disclosure = Disclosure::new("Tool Call")
        .status(status.binding())
        .content(content.binding());

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Disclosure",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 52.min(work.width.saturating_sub(2)).max(30),
                height: 9.min(work.height.saturating_sub(2)).max(6),
            },
            Box::new(disclosure),
        ),
        screen,
    );

    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

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

        let screen: Rect = terminal.size()?.into();
        let result = desktop.handle_event(&ev, screen);

        if result.outcome == atto_ui::composable::EventOutcome::Ignored
            && let Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) = ev
        {
            match code {
                KeyCode::Char('a') => append_chunk(&content),
                KeyCode::Char('d') => status.set(DisclosureStatus::Done),
                KeyCode::Char('e') => status.set(DisclosureStatus::Error),
                KeyCode::Char('r') => status.set(DisclosureStatus::Running),
                _ => {}
            }
        }

        if actions.pop().is_some() {
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
