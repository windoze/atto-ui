use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Frame, Terminal};

use chatty::app::{Desktop, MenuBar};
use chatty::theme::Theme;
use chatty::view::{EventOutcome, View, ViewAction, ViewContext, ViewEventResult};
use chatty::wm::{Window, WindowKind};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// 事件日志视图 - 显示所有接收到的事件
struct EventLogView {
    events: Vec<String>,
    max_events: usize,
}

impl EventLogView {
    fn new() -> Self {
        Self {
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

impl View for EventLogView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
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
                        return ViewEventResult {
                            outcome: EventOutcome::Consumed,
                            action: ViewAction::None,
                        };
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
        ViewEventResult {
            outcome: EventOutcome::Ignored,
            action: ViewAction::None,
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let mut lines: Vec<Line> = Vec::new();

        // 标题
        lines.push(Line::from(vec![
            Span::styled(
                "Event Log ",
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
struct InteractiveView {
    click_count: usize,
    last_click_pos: Option<(u16, u16)>,
}

impl InteractiveView {
    fn new() -> Self {
        Self {
            click_count: 0,
            last_click_pos: None,
        }
    }
}

impl View for InteractiveView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                self.click_count += 1;
                self.last_click_pos = Some((mouse.column, mouse.row));

                // 消费此事件，阻止传播
                return ViewEventResult {
                    outcome: EventOutcome::Consumed,
                    action: ViewAction::None,
                };
            }

        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
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
                    self.click_count.to_string(),
                    Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Last Click: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    if let Some((x, y)) = self.last_click_pos {
                        format!("({}, {})", x, y)
                    } else {
                        "None".to_string()
                    },
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
    // 1. 初始化终端
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        cursor::Hide
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // 2. 创建主题和桌面
    let theme = Theme::dark();
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(theme, menu);

    // 3. 创建事件日志窗口
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
    desktop.add_window(event_log_window, terminal.size()?.into());

    // 4. 创建交互式演示窗口
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
    desktop.add_window(interactive_window, terminal.size()?.into());

    // 5. 主事件循环
    loop {
        // 渲染界面
        terminal.draw(|f| {
            desktop.draw(f);
        })?;

        // 轮询事件
        if event::poll(Duration::from_millis(16))? {
            let ev = event::read()?;
            let screen: Rect = terminal.size()?.into();

            // 让 desktop 处理事件
            let result = desktop.handle_event(&ev, screen);

            // 检查退出条件
            if should_quit(&ev, result.outcome) {
                break;
            }
        }
    }

    // 6. 清理并恢复终端
    cleanup_terminal(&mut terminal)?;
    Ok(())
}

fn should_quit(event: &Event, outcome: EventOutcome) -> bool {
    match event {
        // Ctrl+Q always quits.
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => true,
        // 'q' quits only when the event was not consumed by the UI.
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            ..
        }) => outcome == EventOutcome::Ignored,
        _ => false,
    }
}

fn cleanup_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture,
        cursor::Show
    )?;
    terminal.show_cursor()?;
    Ok(())
}
