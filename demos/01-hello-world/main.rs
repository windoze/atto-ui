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

use chatty::app::{Desktop, MenuBar};
use chatty::theme::Theme;
use chatty::view::{EventOutcome, View, ViewContext, ViewEventResult};
use chatty::wm::{Window, WindowKind};
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

    // 2. 创建主题、菜单和桌面
    let theme = Theme::dark();
    let menu = MenuBar::new(vec![]); // 暂时不创建菜单项
    let mut desktop = Desktop::new(theme, menu);

    // 3. 创建一个简单的窗口
    let window = Window::new(
        WindowKind::Normal, // 普通窗口
        "Hello World",      // 窗口标题
        Rect {
            // 窗口位置和大小
            x: 10,
            y: 5,
            width: 50,
            height: 12,
        },
        Box::new(HelloView), // 窗口内容
    );

    // 4. 将窗口添加到桌面
    let screen: Rect = terminal.size()?.into();
    desktop.add_window(window, screen);

    // 5. 主事件循环
    loop {
        // 渲染界面
        terminal.draw(|f| {
            desktop.draw(f);
        })?;

        // 轮询事件（16ms ≈ 60fps）
        if event::poll(Duration::from_millis(16))? {
            let ev = event::read()?;

            // 让 desktop 处理事件
            let screen: Rect = terminal.size()?.into();
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

/// 判断是否应该退出
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

/// 清理终端
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
