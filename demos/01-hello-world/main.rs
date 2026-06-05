use std::time::Duration;

use anyhow::Result;
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use atto_ui::app::{
    CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop_simple,
};
use atto_ui::composable::{ComponentContext, EventResult};
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_macros::{ComponentProperties, component_properties};

/// 最简单的视图 - 显示 "Hello, World!" 文本
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
        // 创建带边框的块
        let block = Block::default()
            .borders(Borders::NONE)
            .style(ctx.theme.window_bg);

        // 创建文本内容
        let lines = vec![
            Line::raw(""),
            Line::raw(format!("  {}", self.headline.get())),
            Line::raw(""),
            Line::raw("  This is your first Atto-UI application."),
            Line::raw(""),
            Line::raw(format!("  {}", self.footer.get())),
        ];

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(ctx.theme.window_bg);

        frame.render_widget(paragraph, area);
    }
}

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

    run_crossterm_desktop_simple(config, |screen| {
        let theme = Theme::dark();
        let menu = MenuBar::new(vec![]); // 暂时不创建菜单项
        let mut desktop = Desktop::new(theme, menu);

        let window = Window::new(
            WindowKind::Normal, // 普通窗口
            "Hello World",      // 窗口标题
            Rect {
                x: 10,
                y: 5,
                width: 50,
                height: 12,
            },
            Box::new(HelloView::new(
                "Welcome to Atto-UI Framework!",
                "Press 'q' or Ctrl+Q to quit.",
            )),
        );
        desktop.add_window(window, screen);

        Ok(desktop)
    })
}
