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
use chatty::theme::Theme;
use chatty::view::{View, ViewContext, ViewEventResult};
use chatty::views::{EdgeInsets, LayoutParams, Size, VBox};
use chatty::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
struct LineView {
    text: String,
}

impl LineView {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl View for LineView {
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

fn build_scroll_test_view() -> VBox {
    let mut root = VBox::new()
        .with_padding(EdgeInsets::symmetric(1, 1))
        .with_spacing(0)
        .with_scrollable(true);

    root.add_child_with_layout(
        Box::new(LineView::new(
            "Scroll test: ↑↓ PgUp/PgDn Home/End, mouse wheel, drag scrollbar",
        )),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    for i in 0..80u16 {
        root.add_child_with_layout(
            Box::new(LineView::new(format!("{i:03}: line for scrolling"))),
            LayoutParams {
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

    let menu = MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![MenuItem::command("Quit", "app.quit").shortcut("q")],
    )]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Scroll",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 50.min(work.width.saturating_sub(2)).max(20),
                height: 14.min(work.height.saturating_sub(2)).max(8),
            },
            Box::new(build_scroll_test_view()),
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
        let res = desktop.handle_event(&ev, screen);

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

        if let chatty::app::DesktopAction::MenuCommand(cmd) = res.action
            && cmd == "app.quit"
        {
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
