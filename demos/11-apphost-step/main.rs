use std::time::Duration;

use anyhow::Result;
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use atto_ui::app::{AppControl, AppHost, CrosstermAppConfig, CursorMode, Desktop, MenuBar};
use atto_ui::composable::{ComponentContext, EventResult};
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_macros::{ComponentProperties, component_properties};

#[derive(Clone, ComponentProperties)]
struct HelloView {
    headline: Binding<String>,
    footer: Binding<String>,
}

impl HelloView {
    fn new(headline: impl Into<Binding<String>>, footer: impl Into<Binding<String>>) -> Self {
        Self {
            headline: headline.into(),
            footer: footer.into(),
        }
    }
}

#[component_properties]
impl ::atto_ui::composable::Component for HelloView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let block = Block::default()
            .borders(Borders::NONE)
            .style(ctx.theme.window_bg);

        let lines = vec![
            Line::raw(""),
            Line::raw(format!("  {}", self.headline.get())),
            Line::raw(""),
            Line::raw("  AppHost.step() 驱动示例。"),
            Line::raw(""),
            Line::raw(format!("  {}", self.footer.get())),
        ];

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(ctx.theme.window_bg);

        frame.render_widget(paragraph, area);
    }
}

impl ::atto_ui::composable::DragAndDrop for HelloView {}

impl ::atto_ui::composable::Layout for HelloView {}

impl ::atto_ui::composable::Scrollable for HelloView {}

impl ::atto_ui::composable::FocusNav for HelloView {}

impl ::atto_ui::composable::DynamicTree for HelloView {}

impl ::atto_ui::composable::EventHandling for HelloView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }
}

fn main() -> Result<()> {
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    let mut app = AppHost::new(config, |screen| {
        let theme = Theme::dark();
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(theme, menu);

        let window = Window::new(
            WindowKind::Normal,
            "Hello AppHost",
            Rect {
                x: 10,
                y: 5,
                width: 50,
                height: 12,
            },
            Box::new(HelloView::new(
                "Welcome to Atto-UI AppHost!",
                "Press 'q' or Ctrl+Q to quit.",
            )),
        );
        desktop.add_window(window, screen);

        Ok(desktop)
    })?;

    loop {
        if app.step()? == AppControl::Exit {
            break;
        }
    }

    Ok(())
}
