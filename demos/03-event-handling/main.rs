use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;

use atto_ui::app::{
    CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop_simple,
};
use atto_ui::composable::{Component, ComponentContext, EventResult};
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_macros::{ComponentProperties, component_properties};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// 事件日志视图 - 显示所有接收到的事件
#[derive(Clone, ComponentProperties)]
struct EventLogView {
    title: Binding<String>,
    #[component(skip)]
    events: Vec<String>,
    #[component(skip)]
    max_events: usize,
}

impl EventLogView {
    fn new() -> Self {
        Self {
            title: Binding::new("Event Log".to_string()),
            events: vec![
                "Welcome to Event Handling Demo!".to_string(),
                "Try the following:".to_string(),
                "  - Press any key to see keyboard events".to_string(),
                "  - Click mouse to see mouse events".to_string(),
                "  - Scroll mouse wheel to see scroll events".to_string(),
                "  - Press 'c' to clear the log".to_string(),
                "  - Press 'q' or Ctrl+Q to quit".to_string(),
                "".to_string(),
            ],
            max_events: 50,
        }
    }

    fn add_event(&mut self, event_str: String) {
        self.events.push(event_str);
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }
    }

    fn clear(&mut self) {
        self.events.clear();
        self.events.push("Event log cleared".to_string());
    }

    fn format_key_event(&self, key: &KeyEvent) -> String {
        let modifiers = if key.modifiers.is_empty() {
            String::new()
        } else {
            format!("{:?}+", key.modifiers)
        };

        let code = match key.code {
            KeyCode::Char(c) => format!("'{}'", c),
            other => format!("{:?}", other),
        };

        format!("Key: {}{} (kind: {:?})", modifiers, code, key.kind)
    }

    fn format_mouse_event(&self, mouse: &MouseEvent) -> String {
        let kind_str = match mouse.kind {
            MouseEventKind::Down(btn) => format!("Down({:?})", btn),
            MouseEventKind::Up(btn) => format!("Up({:?})", btn),
            MouseEventKind::Drag(btn) => format!("Drag({:?})", btn),
            MouseEventKind::Moved => "Moved".to_string(),
            MouseEventKind::ScrollDown => "ScrollDown".to_string(),
            MouseEventKind::ScrollUp => "ScrollUp".to_string(),
            MouseEventKind::ScrollLeft => "ScrollLeft".to_string(),
            MouseEventKind::ScrollRight => "ScrollRight".to_string(),
        };

        format!("Mouse: {} at ({}, {})", kind_str, mouse.column, mouse.row)
    }
}

#[component_properties]
impl Component for EventLogView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        // 记录事件
        match event {
            Event::Key(key) => {
                // 只记录 Press 事件，避免重复
                if key.kind == KeyEventKind::Press {
                    let event_str = self.format_key_event(key);
                    self.add_event(event_str);

                    // 'c' 键清空日志
                    if key.code == KeyCode::Char('c') && key.modifiers.is_empty() {
                        self.clear();
                        return EventResult::consumed();
                    }
                }
            }
            Event::Mouse(mouse) => {
                // 只记录重要的鼠标事件，避免太多 Moved 事件
                match mouse.kind {
                    MouseEventKind::Moved => {
                        // 跳过移动事件，太频繁
                    }
                    _ => {
                        let event_str = self.format_mouse_event(mouse);
                        self.add_event(event_str);
                    }
                }
            }
            Event::Resize(width, height) => {
                self.add_event(format!("Resize: {}x{}", width, height));
            }
            _ => {
                self.add_event(format!("Other: {:?}", event));
            }
        }

        // 所有事件都标记为 Ignored，让其他组件也能处理
        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let mut lines: Vec<Line> = Vec::new();

        // 标题
        let title = self.title.get();
        lines.push(Line::from(vec![
            Span::styled(
                format!("{title} "),
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({} events)", self.events.len()),
                Style::default().fg(Color::Gray),
            ),
        ]));
        lines.push(Line::raw("─".repeat(area.width as usize)));

        // 事件列表（最新的在底部）
        let start_index = if self.events.len() > area.height.saturating_sub(3) as usize {
            self.events.len() - area.height.saturating_sub(3) as usize
        } else {
            0
        };

        for event_str in &self.events[start_index..] {
            // 根据事件类型着色
            let style = if event_str.starts_with("Key:") {
                Style::default().fg(Color::LightGreen)
            } else if event_str.starts_with("Mouse:") {
                Style::default().fg(Color::LightYellow)
            } else if event_str.starts_with("Resize:") {
                Style::default().fg(Color::LightCyan)
            } else {
                Style::default().fg(Color::Gray)
            };

            lines.push(Line::styled(event_str.clone(), style));
        }

        let paragraph = Paragraph::new(lines).style(ctx.theme.window_bg);

        frame.render_widget(paragraph, area);
    }
}

/// 交互式演示视图 - 展示可点击的按钮
#[derive(Clone, ComponentProperties)]
struct InteractiveView {
    click_count: Binding<usize>,
    last_click_pos: Binding<String>,
}

impl InteractiveView {
    fn new() -> Self {
        Self {
            click_count: Binding::new(0),
            last_click_pos: Binding::new(String::new()),
        }
    }
}

#[component_properties]
impl Component for InteractiveView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
        {
            self.click_count.update(|v| *v = v.saturating_add(1));
            self.last_click_pos
                .set(format!("{}, {}", mouse.column, mouse.row));

            // 消费此事件，阻止传播
            return EventResult::consumed();
        }

        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let click_count = self.click_count.get();
        let last_click = self.last_click_pos.get();
        let last_click = if last_click.is_empty() {
            "None".to_string()
        } else {
            last_click
        };
        let lines = vec![
            Line::raw(""),
            Line::styled(
                "Interactive Demo Area",
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Click Count: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    click_count.to_string(),
                    Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Last Click: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    last_click,
                    Style::default().fg(Color::LightYellow),
                ),
            ]),
            Line::raw(""),
            Line::styled(
                "Click anywhere in this window!",
                Style::default().fg(Color::Yellow),
            ),
            Line::raw(""),
            Line::styled(
                "Note: Clicks here are CONSUMED",
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                "so they won't appear in the log.",
                Style::default().fg(Color::DarkGray),
            ),
        ];

        let paragraph = Paragraph::new(lines).style(ctx.theme.window_bg);

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
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(theme, menu);

        let event_log_window = Window::new(
            WindowKind::Normal,
            "Event Log (Press 'c' to clear)",
            Rect {
                x: 5,
                y: 3,
                width: 55,
                height: 20,
            },
            Box::new(EventLogView::new()),
        );
        desktop.add_window(event_log_window, screen);

        let interactive_window = Window::new(
            WindowKind::Normal,
            "Interactive Area (Click me!)",
            Rect {
                x: 62,
                y: 3,
                width: 40,
                height: 15,
            },
            Box::new(InteractiveView::new()),
        );
        desktop.add_window(interactive_window, screen);

        Ok(desktop)
    })
}
