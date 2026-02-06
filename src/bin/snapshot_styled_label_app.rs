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
use atto_ui::composable::{Component, VStack};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::{Label, StyledLabel};
use atto_ui::wm::{Window, WindowKind};

const MARKUP: &str = "**BOLD** *ITALIC* __UNDER__ ~~STRIKE~~ [LINK](https://example.com)";

fn build_view(clicked_text: Property<String>) -> Box<dyn Component> {
    let clicked = clicked_text.clone();
    let styled = StyledLabel::new(MARKUP).on_link(move |url| {
        clicked.set(format!("Clicked: {url}"));
    });

    let status = Label::new(clicked_text.binding());

    let root = VStack::new().child(styled).child(status);
    Box::new(root)
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

    let clicked_text = Property::new("Clicked: (none)".to_string());

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "StyledLabel",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 70.min(work.width.saturating_sub(2)).max(30),
                height: 8.min(work.height.saturating_sub(2)).max(6),
            },
            build_view(clicked_text),
        ),
        screen,
    );

    loop {
        terminal.draw(|f| desktop.draw(f))?;

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
