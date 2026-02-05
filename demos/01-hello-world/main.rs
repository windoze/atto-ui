use std::time::Duration;

use anyhow::Result;
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use atto_ui::app::{CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop_simple};
use atto_ui::theme::Theme;
use atto_ui::view::{View, ViewContext, ViewEventResult};
use atto_ui::wm::{Window, WindowKind};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

/// 最简单的视图 - 显示 "Hello, World!" 文本
struct HelloView;

impl View for HelloView {
    fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        // 创建带边框的块
        let block = Block::default()
            .borders(Borders::NONE)
            .style(ctx.theme.window_bg);

        // 创建文本内容
        let lines = vec![
            Line::raw(""),
            Line::raw("  Welcome to Chatty Framework!"),
            Line::raw(""),
            Line::raw("  This is your first Chatty application."),
            Line::raw(""),
            Line::raw("  Press 'q' or Ctrl+Q to quit."),
        ];

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(ctx.theme.window_bg);

        frame.render_widget(paragraph, area);
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
            Box::new(HelloView),
        );
        desktop.add_window(window, screen);

        Ok(desktop)
    })
}
