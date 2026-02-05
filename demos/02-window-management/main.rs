use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop,
};
use atto_ui::theme::Theme;
use atto_ui::view::{EventOutcome, View, ViewAction, ViewContext, ViewEventResult};
use atto_ui::wm::{Window, WindowKind};
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
            outcome: atto_ui::view::EventOutcome::Ignored,
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
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    let window_counter = Rc::new(Cell::new(0usize));
    let window_counter_build = Rc::clone(&window_counter);
    let window_counter_events = Rc::clone(&window_counter);

    run_crossterm_desktop(
        config,
        move |screen| {
            let theme = Theme::dark();
            let menu = MenuBar::new(vec![]);
            let mut desktop = Desktop::new(theme, menu);

            let n = window_counter_build.get();
            let window = Window::new(
                WindowKind::Normal,
                format!("Window #{}", n),
                Rect {
                    x: 5,
                    y: 3,
                    width: 45,
                    height: 18,
                },
                Box::new(WindowInfoView::new("Normal", n)),
            );
            desktop.add_window(window, screen);
            window_counter_build.set(n + 1);

            Ok(desktop)
        },
        |_desktop, _screen| Ok(AppControl::Continue),
        move |desktop, ev, screen, result| {
            if result.outcome != EventOutcome::Ignored {
                return Ok(AppControl::Continue);
            }

            let Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            }) = ev
            else {
                return Ok(AppControl::Continue);
            };

            match *code {
                KeyCode::Char('n') => {
                    let n = window_counter_events.get();
                    let window = Window::new(
                        WindowKind::Normal,
                        format!("Window #{}", n),
                        Rect {
                            x: 10 + (n as u16 % 10) * 2,
                            y: 5 + (n as u16 % 5),
                            width: 45,
                            height: 18,
                        },
                        Box::new(WindowInfoView::new("Normal", n)),
                    );
                    desktop.add_window(window, screen);
                    window_counter_events.set(n + 1);
                }
                KeyCode::Char('f') => {
                    let n = window_counter_events.get();
                    let window = Window::new(
                        WindowKind::Floating,
                        format!("Floating #{}", n),
                        Rect {
                            x: 15 + (n as u16 % 8) * 2,
                            y: 7 + (n as u16 % 4),
                            width: 35,
                            height: 12,
                        },
                        Box::new(WindowInfoView::new("Floating", n)),
                    );
                    desktop.add_window(window, screen);
                    window_counter_events.set(n + 1);
                }
                KeyCode::Char('m') => {
                    let work_area = Desktop::layout(screen).work_area;
                    let dialog_width = 40;
                    let dialog_height = 10;
                    let window = Window::new(
                        WindowKind::Modal,
                        "Modal Dialog",
                        Rect {
                            x: work_area.x + (work_area.width.saturating_sub(dialog_width)) / 2,
                            y: work_area.y + (work_area.height.saturating_sub(dialog_height)) / 2,
                            width: dialog_width,
                            height: dialog_height,
                        },
                        Box::new(ModalView),
                    );
                    desktop.add_window(window, screen);
                }
                KeyCode::Char('c') => {
                    if let Some(id) = desktop.wm.focused() {
                        desktop.wm.request_close(id);
                    }
                }
                KeyCode::Tab => {
                    desktop.wm.focus_next();
                }
                _ => {}
            }

            Ok(AppControl::Continue)
        },
    )
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
                outcome: atto_ui::view::EventOutcome::Consumed,
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
