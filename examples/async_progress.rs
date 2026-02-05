/// 演示如何使用 mpsc::channel 从后台任务更新 UI（如下载进度条）
///
/// 运行: cargo run --example async_progress
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::{Frame, Terminal};

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::reactive::Property;
use atto_ui::theme::Theme;
use atto_ui::composable::{Component, ComponentContext, EventResult};
use atto_ui::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
enum AppAction {
    Quit,
    StartDownload,
    UpdateProgress(f64),      // 0.0 到 1.0
    DownloadComplete(String), // 完成消息
}

struct ProgressView {
    progress: Property<f64>,
    status: Property<String>,
}

impl ProgressView {
    fn new() -> Self {
        Self {
            progress: Property::new(0.0),
            status: Property::new("Ready to download".to_string()),
        }
    }
}

impl Component for ProgressView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let progress = self.progress.get();
        let status = self.status.get();

        let lines = vec![
            Line::raw(""),
            Line::raw(format!("Status: {}", status)),
            Line::raw(""),
            Line::raw(format!("Progress: {:.1}%", progress * 100.0)),
            Line::raw(""),
            Line::raw("Press 's' to start download"),
            Line::raw("Press 'q' to quit"),
        ];

        let paragraph = Paragraph::new(lines)
            .style(ctx.theme.window_bg)
            .block(Block::default().borders(Borders::NONE));

        let text_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 7.min(area.height),
        };
        frame.render_widget(paragraph, text_area);

        // 进度条
        if area.height >= 9 {
            let gauge_area = Rect {
                x: area.x + 2,
                y: area.y + 8,
                width: area.width.saturating_sub(4),
                height: 1,
            };

            let gauge = Gauge::default()
                .ratio(progress)
                .label(format!("{:.0}%", progress * 100.0))
                .gauge_style(Style::default().fg(Color::Green));

            frame.render_widget(gauge, gauge_area);
        }
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

    // 2. 创建桌面
    let theme = Theme::dark();
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(theme, menu);

    // 3. 创建视图
    let view = ProgressView::new();
    let progress_binding = view.progress.clone();
    let status_binding = view.status.clone();

    let window = Window::new(
        WindowKind::Normal,
        "Async Download Demo",
        Rect {
            x: 10,
            y: 5,
            width: 60,
            height: 12,
        },
        Box::new(view),
    );
    desktop.add_window(window, terminal.size()?.into());

    // 4. 创建 channel 用于后台任务通信
    let (action_sender, action_receiver) = mpsc::channel::<AppAction>();

    // 5. 主事件循环
    let result = run_event_loop(
        &mut terminal,
        &mut desktop,
        action_sender,
        action_receiver,
        progress_binding,
        status_binding,
    );

    // 6. 清理
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture,
        cursor::Show
    )?;
    terminal.show_cursor()?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    desktop: &mut Desktop,
    action_sender: mpsc::Sender<AppAction>,
    action_receiver: mpsc::Receiver<AppAction>,
    progress: Property<f64>,
    status: Property<String>,
) -> Result<()> {
    loop {
        terminal.draw(|f| desktop.draw(f))?;

        // 💡 关键：同时监听终端事件和应用动作
        // 1. 先检查 channel（非阻塞）
        let mut handled_action = false;
        while let Ok(action) = action_receiver.try_recv() {
            match action {
                AppAction::Quit => return Ok(()),
                AppAction::StartDownload => {
                    status.set("Downloading...".to_string());
                    start_download_task(action_sender.clone());
                }
                AppAction::UpdateProgress(p) => {
                    progress.set(p);
                }
                AppAction::DownloadComplete(msg) => {
                    progress.set(1.0);
                    status.set(msg);
                }
            }
            handled_action = true;
        }

        // 2. 轮询终端事件（带超时）
        let timeout = if handled_action {
            Duration::from_millis(1) // 刚处理过动作，快速轮询
        } else {
            Duration::from_millis(50) // 空闲状态，正常超时
        };

        if event::poll(timeout)? {
            let ev = event::read()?;
            let screen: Rect = terminal.size()?.into();
            let _result = desktop.handle_event(&ev, screen);

            // 处理用户输入
            if let Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) = ev
            {
                match code {
                    KeyCode::Char('q') => {
                        action_sender.send(AppAction::Quit).ok();
                    }
                    KeyCode::Char('s') => {
                        action_sender.send(AppAction::StartDownload).ok();
                    }
                    _ => {}
                }
            }
        }
    }
}

fn start_download_task(sender: mpsc::Sender<AppAction>) {
    thread::spawn(move || {
        // 模拟下载任务
        for i in 0..=100 {
            thread::sleep(Duration::from_millis(30));

            let progress = i as f64 / 100.0;
            sender.send(AppAction::UpdateProgress(progress)).ok();
        }

        sender
            .send(AppAction::DownloadComplete(
                "Download complete!".to_string(),
            ))
            .ok();
    });
}
