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
use atto_ui::composable::{Component, TabWindow};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::widgets::{Label, TabView};
use atto_ui::wm::{Window, WindowButtons, WindowKind};

fn build_tab_view() -> Box<dyn Component> {
    let tabs = TabView::new()
        .tab("Tab01-LongName", Label::new("Tab view tab 1"))
        .tab("Tab02-LongName", Label::new("Tab view tab 2"))
        .tab("Tab03-LongName", Label::new("Tab view tab 3"))
        .tab("Tab04-LongName", Label::new("Tab view tab 4"))
        .tab("Tab05-LongName", Label::new("Tab view tab 5"))
        .tab("Tab06-LongName", Label::new("Tab view tab 6"))
        .tab("Tab07-LongName", Label::new("Tab view tab 7"));

    Box::new(tabs)
}

fn build_tab_window() -> Box<dyn Component> {
    let mut tabs = TabWindow::new();
    for idx in 1..=7 {
        tabs.add_tab(
            format!("Win{:02}-LongName", idx),
            Box::new(Label::new(format!("Tab window tab {idx}"))),
        );
    }

    Box::new(tabs)
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

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    let window_width = 32.min(work.width.saturating_sub(4)).max(20);
    let window_height = 7.min(work.height.saturating_sub(6)).max(6);
    let tab_view_rect = Rect {
        x: work.x.saturating_add(2),
        y: work.y.saturating_add(1),
        width: window_width,
        height: window_height,
    };
    let tab_window_rect = Rect {
        x: tab_view_rect.x,
        y: tab_view_rect
            .y
            .saturating_add(window_height)
            .saturating_add(3),
        width: window_width,
        height: window_height,
    };

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "TabViewOverflow",
            tab_view_rect,
            build_tab_view(),
        ),
        screen,
    );

    let tab_window = Window::new(
        WindowKind::Normal,
        "TabWindowOverflow",
        tab_window_rect,
        build_tab_window(),
    );
    tab_window.decorations.update(|d| {
        d.buttons = WindowButtons {
            minimize: false,
            maximize: false,
            close: false,
        }
    });
    desktop.add_window(tab_window, screen);

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
