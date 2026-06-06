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
use atto_ui::composable::{Component, ScrollbarVisibility};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_markdown::MarkdownViewer;

const MARKDOWN: &str = r#"
```text
CODE-LINE-00
CODE-LINE-01
CODE-LINE-02
CODE-LINE-03
CODE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 CODE-HSCROLL-END
CODE-LINE-04
CODE-LINE-05
CODE-LINE-06
CODE-LINE-07
CODE-LINE-08
CODE-LINE-09
```

| ColA | ColB |
| --- | --- |
| ROW-00 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-01 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-02 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-03 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-04 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-05 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-06 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-07 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-08 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
| ROW-09 | TABLE-HSCROLL-BEGIN 0123456789 0123456789 0123456789 0123456789 TABLE-HSCROLL-END |
"#;

const BLOCK_MARKDOWN: &str = r#"
# T19 Heading

Intro paragraph with **strong text** and *emphasis*.

- parent item
  - nested child item
  - nested child two

> quoted line
> continued quote

```rust
fn main() {
    println!("t19");
}
```

| Feature | Status |
| --- | --- |
| heading | ok |
| table | ok |
"#;

fn build_markdown_view(markdown: &'static str) -> Box<dyn Component> {
    let viewer = MarkdownViewer::new(markdown)
        .wrap_width(32)
        .vertical_scrollbar(ScrollbarVisibility::Never)
        .code_block_max_height(6)
        .table_max_height(6);
    Box::new(viewer)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let blocks_fixture = args.iter().any(|arg| arg == "--blocks");
    let markdown = if blocks_fixture {
        BLOCK_MARKDOWN
    } else {
        MARKDOWN
    };

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

    let window_height = if blocks_fixture { 24 } else { 18 };
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Markdown",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 70.min(work.width.saturating_sub(2)).max(30),
                height: window_height.min(work.height.saturating_sub(2)).max(10),
            },
            build_markdown_view(markdown),
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
