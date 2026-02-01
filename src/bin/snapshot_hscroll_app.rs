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
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};

use chatty::app::{Desktop, MenuBar, MenuItem, MenuSpec};
use chatty::reactive::EventQueue;
use chatty::theme::Theme;
use chatty::view::{View, ViewContext, ViewEventResult};
use chatty::views::{EdgeInsets, HBox, LayoutParams, Size};
use chatty::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
struct CellView {
    text: String,
}

impl CellView {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl View for CellView {
    fn desired_width(&self) -> Option<u16> {
        Some(self.text.len().min(u16::MAX as usize) as u16)
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let style = if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        frame.render_widget(Paragraph::new(Line::styled(self.text.clone(), style)), area);
    }
}

fn build_hscroll_test_view() -> HBox {
    let mut root = HBox::new()
        .with_padding(EdgeInsets::symmetric(1, 1))
        .with_spacing(1)
        .with_scrollable(true);

    for i in 0..40u16 {
        root.add_child_with_layout(
            Box::new(CellView::new(format!("[col-{i:02}]"))),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }

    root
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
            "HScroll",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 50.min(work.width.saturating_sub(2)).max(20),
                height: 8.min(work.height.saturating_sub(2)).max(6),
            },
            Box::new(build_hscroll_test_view()),
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
