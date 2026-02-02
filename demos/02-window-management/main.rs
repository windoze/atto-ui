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
use ratatui::{Frame, Terminal};

use chatty::app::{Desktop, DesktopAction, MenuBar};
use chatty::theme::Theme;
use chatty::view::{EventOutcome, View, ViewAction, ViewContext, ViewEventResult};
use chatty::wm::{Window, WindowKind};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// 显示窗口信息的视图
struct WindowInfoView {
    window_type: String,
    window_number: usize,
}

impl WindowInfoView {
    fn new(window_type: &str, window_number: usize) -> Self {
        Self {
            window_type: window_type.to_string(),
            window_number,
        }
    }
}

impl View for WindowInfoView {
    fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        ViewEventResult {
            outcome: chatty::view::EventOutcome::Ignored,
            action: ViewAction::None,
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let lines = vec![
            Line::raw(""),
            Line::from(vec![
                Span::styled("Window Type: ", Style::default().fg(Color::Gray)),
                Span::styled(&self.window_type, Style::default().fg(Color::LightBlue)),
            ]),
            Line::from(vec![
                Span::styled("Window #", Style::default().fg(Color::Gray)),
                Span::styled(
                    self.window_number.to_string(),
                    Style::default().fg(Color::LightBlue),
                ),
            ]),
            Line::raw(""),
            Line::styled("Press:", Style::default().fg(Color::Yellow)),
            Line::raw("  n - New normal window"),
            Line::raw("  f - New floating window"),
            Line::raw("  m - Open modal dialog"),
            Line::raw("  c - Close this window"),
            Line::raw("  Tab - Next window"),
            Line::raw("  q - Quit"),
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

    // 3. 创建初始窗口
    let mut window_counter = 0;

    let window = Window::new(
        WindowKind::Normal,
        format!("Window #{}", window_counter),
        Rect {
            x: 5,
            y: 3,
            width: 45,
            height: 18,
        },
        Box::new(WindowInfoView::new("Normal", window_counter)),
    );
    desktop.add_window(window, terminal.size()?.into());
    window_counter += 1;

    // 4. 主事件循环
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

            // 处理 desktop 返回的动作
            if let DesktopAction::CloseWindow(id) = result.action {
                desktop.wm.close(id);
            }

            // 检查退出条件
            if should_quit(&ev, result.outcome) {
                break;
            }

            // 处理应用级别的快捷键
            if result.outcome == EventOutcome::Ignored
                && let Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::NONE,
                    kind: KeyEventKind::Press,
                    ..
                }) = ev
            {
                match code {
                    KeyCode::Char('n') => {
                        // 创建新的普通窗口
                        let window = Window::new(
                            WindowKind::Normal,
                            format!("Window #{}", window_counter),
                            Rect {
                                x: 10 + (window_counter as u16 % 10) * 2,
                                y: 5 + (window_counter as u16 % 5),
                                width: 45,
                                height: 18,
                            },
                            Box::new(WindowInfoView::new("Normal", window_counter)),
                        );
                        desktop.add_window(window, screen);
                        window_counter += 1;
                    }
                    KeyCode::Char('f') => {
                        // 创建浮动窗口
                        let window = Window::new(
                            WindowKind::Floating,
                            format!("Floating #{}", window_counter),
                            Rect {
                                x: 15 + (window_counter as u16 % 8) * 2,
                                y: 7 + (window_counter as u16 % 4),
                                width: 35,
                                height: 12,
                            },
                            Box::new(WindowInfoView::new("Floating", window_counter)),
                        );
                        desktop.add_window(window, screen);
                        window_counter += 1;
                    }
                    KeyCode::Char('m') => {
                        // 创建模态对话框
                        let work_area = Desktop::layout(screen).work_area;
                        let dialog_width = 40;
                        let dialog_height = 10;
                        let window = Window::new(
                            WindowKind::Modal,
                            "Modal Dialog",
                            Rect {
                                x: work_area.x + (work_area.width.saturating_sub(dialog_width)) / 2,
                                y: work_area.y
                                    + (work_area.height.saturating_sub(dialog_height)) / 2,
                                width: dialog_width,
                                height: dialog_height,
                            },
                            Box::new(ModalView),
                        );
                        desktop.add_window(window, screen);
                    }
                    KeyCode::Char('c') => {
                        // 关闭当前聚焦的窗口
                        if let Some(id) = desktop.wm.focused() {
                            desktop.wm.request_close(id);
                        }
                    }
                    KeyCode::Tab => {
                        // 切换到下一个窗口
                        desktop.wm.focus_next();
                    }
                    _ => {}
                }
            }
        }
    }

    // 5. 清理并恢复终端
    cleanup_terminal(&mut terminal)?;
    Ok(())
}

/// 模态对话框视图
struct ModalView;

impl View for ModalView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        // 按 Esc 或 Enter 关闭对话框
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc | KeyCode::Enter,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return ViewEventResult {
                outcome: chatty::view::EventOutcome::Consumed,
                action: ViewAction::CloseWindow,
            };
        }
        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let lines = vec![
            Line::raw(""),
            Line::styled(
                "  This is a Modal Dialog",
                Style::default().fg(Color::White),
            ),
            Line::raw(""),
            Line::styled(
                "  Modal windows block interaction",
                Style::default().fg(Color::Gray),
            ),
            Line::styled("  with other windows.", Style::default().fg(Color::Gray)),
            Line::raw(""),
            Line::styled(
                "  Press Enter or Esc to close",
                Style::default().fg(Color::Yellow),
            ),
        ];

        let paragraph = Paragraph::new(lines).style(ctx.theme.window_bg);

        frame.render_widget(paragraph, area);
    }
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
