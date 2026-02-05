use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::composable::{Component, Divider, ForEach, Text, VStack};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};

fn build_fruit_list(fruits: Property<Vec<String>>) -> Box<dyn Component> {
    Box::new(
        VStack::new()
            .padding(1)
            .spacing(0)
            .child(Text::new("Fruit List (Simple ForEach)"))
            .child(Divider::horizontal())
            .child(
                ForEach::new(fruits.binding(), |fruit, idx| {
                    Text::new(format!("{idx}. {fruit}"))
                })
                .spacing(0),
            ),
    )
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

    // 创建水果列表数据
    let fruits = Property::new(vec![
        "Apple".to_string(),
        "Banana".to_string(),
        "Cherry".to_string(),
        "Durian".to_string(),
    ]);

    let actions: EventQueue<()> = EventQueue::new();
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "ForEach Demo",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 40.min(work.width.saturating_sub(2)).max(20),
                height: 20.min(work.height.saturating_sub(2)).max(12),
            },
            build_fruit_list(fruits.clone()),
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

        match ev {
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                break;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('a'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                // 添加一个新水果
                let mut current_fruits = fruits.get();
                current_fruits.push("Elderberry".to_string());
                fruits.set(current_fruits);
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('r'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                // 删除第一个水果
                let mut current_fruits = fruits.get();
                if !current_fruits.is_empty() {
                    current_fruits.remove(0);
                    fruits.set(current_fruits);
                }
            }
            _ => {}
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
