use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use atto_ui::app::{Desktop, MenuBar, MenuItem, MenuSpec};
use atto_ui::composable::{
    Component, ComponentContext, EdgeInsets, EventResult, LayoutParams, Size, Text, VStack,
};
use atto_ui::reactive::{Binding, EventQueue};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};

struct TallClickTarget {
    status: Binding<String>,
}

impl TallClickTarget {
    fn new() -> Self {
        Self {
            status: Binding::new("idle".to_string()),
        }
    }
}

impl ::atto_ui::composable::Component for TallClickTarget {
    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let status = self.status.get();
        let style = ctx.theme.widget.normal;
        let lines: Vec<Line<'_>> = (0..area.height)
            .map(|row| Line::styled(format!("Tall child row {row:02} | {status}"), style))
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl ::atto_ui::composable::DragAndDrop for TallClickTarget {}

impl ::atto_ui::composable::Layout for TallClickTarget {
    fn desired_height(&self) -> Option<u16> {
        Some(40)
    }
}

impl ::atto_ui::composable::Scrollable for TallClickTarget {}

impl ::atto_ui::composable::FocusNav for TallClickTarget {}

impl ::atto_ui::composable::DynamicTree for TallClickTarget {}

impl ::atto_ui::composable::EventHandling for TallClickTarget {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        if matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            self.status.set(format!("clicked row {:02}", m.row));
            return EventResult::changed();
        }
        EventResult::ignored()
    }
}

fn build_scroll_test_view() -> Box<dyn Component> {
    let root = (0..80u16).fold(
        VStack::new()
            .padding_insets(EdgeInsets::symmetric(1, 1))
            .spacing(0u16)
            .scrollable(true)
            .child_with_layout(
                Text::new("Scroll test: ↑↓ PgUp/PgDn Home/End, mouse wheel, drag scrollbar"),
                LayoutParams {
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            ),
        |v, i| {
            v.child_with_layout(
                Text::new(format!("{i:03}: line for scrolling")),
                LayoutParams {
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            )
        },
    );

    Box::new(root)
}

fn build_long_child_test_view() -> Box<dyn Component> {
    Box::new(
        VStack::new()
            .padding_insets(EdgeInsets::symmetric(1, 1))
            .spacing(0u16)
            .scrollable(true)
            .child_with_layout(
                TallClickTarget::new(),
                LayoutParams {
                    height: Size::Fixed(40),
                    ..LayoutParams::default()
                },
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

    let long_child_mode = std::env::args().any(|arg| arg == "--long-child");
    let view = if long_child_mode {
        build_long_child_test_view()
    } else {
        build_scroll_test_view()
    };

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
            view,
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
