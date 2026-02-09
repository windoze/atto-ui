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
use atto_ui::composable::Component;
use atto_ui::reactive::{Binding, EventQueue};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_file_tree::{FileTree, FileTreeGlyphs, FileTreeNode, FileTreeNodeId};

fn build_file_tree_view() -> Box<dyn Component> {
    let roots = vec![
        FileTreeNode::dir(
            1,
            "src",
            vec![
                FileTreeNode::file(2, "main.rs"),
                FileTreeNode::file(3, "lib.rs"),
            ],
        )
        .with_expanded(true),
        FileTreeNode::dir(4, "assets", vec![FileTreeNode::file(5, "logo.png")])
            .with_expanded(false),
        FileTreeNode::file(6, "README.md"),
        FileTreeNode::file(7, "Cargo.toml"),
    ];

    let selection: Binding<Option<FileTreeNodeId>> = Binding::new(None);
    let glyphs = FileTreeGlyphs::default()
        .with_extension("rs", "rs")
        .with_extension("md", "md")
        .with_extension("toml", "tm")
        .with_extension("png", "img");

    Box::new(
        FileTree::new("Files", roots, selection)
            .glyphs(glyphs)
            .with_min_height(8),
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
            "File Tree",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 50.min(work.width.saturating_sub(2)).max(24),
                height: 16.min(work.height.saturating_sub(2)).max(10),
            },
            build_file_tree_view(),
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
