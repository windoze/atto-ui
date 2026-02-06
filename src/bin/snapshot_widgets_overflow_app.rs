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
use atto_ui::composable::{Component, EdgeInsets, LayoutParams, Size, VStack};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::{Label, ListBox, TableView, TextBox};
use atto_ui::wm::{Window, WindowKind};

const PLACEHOLDER: &str = "PLACEHOLDER";

fn build_view() -> Box<dyn Component> {
    let empty_text = Property::new(String::new());
    let text = Property::new("alpha beta gamma".to_string());
    let list_selection = Property::new(0usize);
    let table_selection = Property::new(0usize);

    let items = (0..30).map(|i| format!("Item {i:02}")).collect::<Vec<_>>();
    let rows = (0..30)
        .map(|i| vec![format!("K{i:02}"), format!("V{i:02}")])
        .collect::<Vec<_>>();

    let row_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    Box::new(
        VStack::new()
            .padding_insets(EdgeInsets::all(1))
            .spacing(0)
            .child_with_layout(
                Label::new("Widgets overflow test: list/table scrollbars + textbox selection."),
                row_layout,
            )
            .child_with_layout(
                TextBox::new("Empty", empty_text.binding()).placeholder(PLACEHOLDER),
                row_layout,
            )
            .child_with_layout(TextBox::new("Text", text.binding()), row_layout)
            .child_with_layout(
                ListBox::new("List", items, list_selection.binding()).height(5u16),
                row_layout,
            )
            .child_with_layout(
                TableView::new(
                    "Table",
                    vec!["Key".into(), "Value".into()],
                    rows,
                    table_selection.binding(),
                )
                .height(6u16),
                row_layout,
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

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Widgets",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 60.min(work.width.saturating_sub(2)).max(30),
                height: 20.min(work.height.saturating_sub(2)).max(12),
            },
            build_view(),
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
