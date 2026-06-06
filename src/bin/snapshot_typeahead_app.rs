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
use atto_ui::composable::{Text, VStack};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::CommandPalette;
use atto_ui::wm::{Window, WindowKind};

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

    let query = Property::new(String::new());
    let accepted = Property::new(String::new());
    let commands = Property::new(vec![
        "/open-file".to_string(),
        "/search-files".to_string(),
        "/switch-project".to_string(),
        "/help".to_string(),
        "@src/lib.rs".to_string(),
        "@src/widgets/typeahead.rs".to_string(),
        "@README.md".to_string(),
    ]);

    let accepted_status = accepted.binding();
    let palette = CommandPalette::new("Command Palette", query.binding(), commands.binding())
        .accepted(accepted.binding())
        .height(9u16)
        .max_results(6usize);
    let content = VStack::new()
        .with_spacing(1)
        .child(palette)
        .child(Text::from_fn(move || {
            format!("Accepted: {}", accepted_status.get())
        }));

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "TypeAhead",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 56.min(work.width.saturating_sub(2)).max(36),
                height: 14.min(work.height.saturating_sub(2)).max(10),
            },
            Box::new(content),
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
        let _ = desktop.handle_event(&ev, screen);

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
