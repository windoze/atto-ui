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
use atto_ui::composable::{ComponentContext, EventOutcome, EventResult};
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_macros::{ComponentProperties, component_properties};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// 显示窗口信息的视图
#[derive(Clone, ComponentProperties)]
struct WindowInfoView {
    window_type: Binding<String>,
    window_number: Binding<usize>,
}

impl WindowInfoView {
    fn new(
        window_type: impl Into<Binding<String>>,
        window_number: impl Into<Binding<usize>>,
    ) -> Self {
        Self {
            window_type: window_type.into(),
            window_number: window_number.into(),
        }
    }
}

#[component_properties]
impl ::atto_ui::composable::Component for WindowInfoView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let window_type = self.window_type.get();
        let window_number = self.window_number.get();
        let lines = vec![
            Line::raw(""),
            Line::from(vec![
                Span::styled("Window Type: ", Style::default().fg(Color::Gray)),
                Span::styled(window_type, Style::default().fg(Color::LightBlue)),
            ]),
            Line::from(vec![
                Span::styled("Window #", Style::default().fg(Color::Gray)),
                Span::styled(
                    window_number.to_string(),
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

impl ::atto_ui::composable::DragAndDrop for WindowInfoView {}

impl ::atto_ui::composable::Layout for WindowInfoView {}

impl ::atto_ui::composable::Scrollable for WindowInfoView {}

impl ::atto_ui::composable::FocusNav for WindowInfoView {}

impl ::atto_ui::composable::DynamicTree for WindowInfoView {}

impl ::atto_ui::composable::EventHandling for WindowInfoView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
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
                        Box::new(ModalView::new(
                            "This is a Modal Dialog",
                            "Modal windows block interaction",
                        )),
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
#[derive(Clone, ComponentProperties)]
struct ModalView {
    title: Binding<String>,
    subtitle: Binding<String>,
}

impl ModalView {
    fn new(title: impl Into<Binding<String>>, subtitle: impl Into<Binding<String>>) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
        }
    }
}

#[component_properties]
impl ::atto_ui::composable::Component for ModalView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let title = self.title.get();
        let subtitle = self.subtitle.get();
        let lines = vec![
            Line::raw(""),
            Line::styled(format!("  {title}"), Style::default().fg(Color::White)),
            Line::raw(""),
            Line::styled(format!("  {subtitle}"), Style::default().fg(Color::Gray)),
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

impl ::atto_ui::composable::DragAndDrop for ModalView {}

impl ::atto_ui::composable::Layout for ModalView {}

impl ::atto_ui::composable::Scrollable for ModalView {}

impl ::atto_ui::composable::FocusNav for ModalView {}

impl ::atto_ui::composable::DynamicTree for ModalView {}

impl ::atto_ui::composable::EventHandling for ModalView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        // 按 Esc 或 Enter 关闭对话框
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc | KeyCode::Enter,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return EventResult::close_window();
        }
        EventResult::ignored()
    }
}
