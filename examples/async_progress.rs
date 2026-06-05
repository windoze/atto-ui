/// 演示如何使用 mpsc::channel 从后台任务更新 UI（如下载进度条）
///
/// 运行: cargo run --example async_progress
use std::thread;
use std::time::Duration;

use anyhow::Result;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use atto_ui::app::{
    AppControl, CrosstermAppConfig, Desktop, MenuBar, run_crossterm_desktop_with_actions,
};
use atto_ui::composable::{ComponentContext, EventOutcome, EventResult};
use atto_ui::reactive::{Binding, EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_macros::{ComponentProperties, component_properties};

#[derive(Clone, Debug)]
enum AppAction {
    StartDownload,
    UpdateProgress(f64),      // 0.0 到 1.0
    DownloadComplete(String), // 完成消息
}

#[derive(Clone, ComponentProperties)]
struct ProgressView {
    progress: Binding<f64>,
    status: Binding<String>,
}

impl ProgressView {
    fn new(progress: impl Into<Binding<f64>>, status: impl Into<Binding<String>>) -> Self {
        Self {
            progress: progress.into(),
            status: status.into(),
        }
    }
}

#[component_properties]
impl ::atto_ui::composable::Component for ProgressView {
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

impl ::atto_ui::composable::Layout for ProgressView {}

impl ::atto_ui::composable::Scrollable for ProgressView {}

impl ::atto_ui::composable::FocusNav for ProgressView {}

impl ::atto_ui::composable::DynamicTree for ProgressView {}

impl ::atto_ui::composable::EventHandling for ProgressView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }
}

fn main() -> Result<()> {
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true);

    // Shared UI state (updated on the main thread in `on_action`).
    let progress = Property::new(0.0);
    let status = Property::new("Ready to download".to_string());
    let progress_for_view = progress.binding();
    let status_for_view = status.binding();
    let progress_for_actions = progress;
    let status_for_actions = status;

    // Background actions are bridged to the main thread via a standard library channel.
    let (action_sender, action_receiver) = EventQueue::<AppAction>::channel();
    let sender_for_events = action_sender.clone();
    let sender_for_actions = action_sender;

    run_crossterm_desktop_with_actions(
        config,
        move |screen| {
            let theme = Theme::dark();
            let menu = MenuBar::new(vec![]);
            let mut desktop = Desktop::new(theme, menu);

            let window = Window::new(
                WindowKind::Normal,
                "Async Download Demo",
                Rect {
                    x: 10,
                    y: 5,
                    width: 60,
                    height: 12,
                },
                Box::new(ProgressView::new(
                    progress_for_view.clone(),
                    status_for_view.clone(),
                )),
            );
            desktop.add_window(window, screen);

            Ok(desktop)
        },
        action_receiver,
        move |_desktop, action, _screen| {
            match action {
                AppAction::StartDownload => {
                    status_for_actions.set("Downloading...".to_string());
                    start_download_task(sender_for_actions.clone());
                }
                AppAction::UpdateProgress(p) => {
                    progress_for_actions.set(p);
                }
                AppAction::DownloadComplete(msg) => {
                    progress_for_actions.set(1.0);
                    status_for_actions.set(msg);
                }
            }
            Ok(AppControl::Continue)
        },
        |_desktop, _screen| Ok(AppControl::Continue),
        move |_desktop, event, _screen, result| {
            // Application-level shortcuts: only run if the event was not handled by the UI.
            if result.outcome != EventOutcome::Ignored {
                return Ok(AppControl::Continue);
            }

            let Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            }) = event
            else {
                return Ok(AppControl::Continue);
            };

            if let KeyCode::Char('s') = *code {
                sender_for_events.send(AppAction::StartDownload).ok();
            }

            Ok(AppControl::Continue)
        },
    )
}

fn start_download_task(sender: std::sync::mpsc::Sender<AppAction>) {
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
