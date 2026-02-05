use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;

#[derive(Clone)]
pub struct Button {
    label: Binding<String>,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: Binding<bool>,
}

impl Button {
    pub fn new(label: impl Into<Binding<String>>) -> Self {
        Self {
            label: label.into(),
            on_click: None,
            enabled: true.into(),
        }
    }

    pub fn on_click<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_click = Some(Arc::new(callback));
        self
    }

    pub fn label(mut self, label: impl Into<Binding<String>>) -> Self {
        self.label = label.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    fn trigger(&self) {
        if let Some(cb) = &self.on_click {
            cb();
        }
    }
}

impl Component for Button {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        3
    }

    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }

    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.trigger();
                    return EventResult::submitted();
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent {
                code: KeyCode::Enter | KeyCode::Char(' '),
                ..
            }) => {
                self.trigger();
                EventResult::submitted()
            }
            Event::Key(KeyEvent { .. }) => EventResult::ignored(),
            _ => EventResult::ignored(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        Some(3)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let enabled = self.enabled.get();
        let style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .border_set(ctx.theme.border_set(false));
        let text = Line::styled(format!(" {} ", self.label.get()), style);
        let p = Paragraph::new(text).block(block);
        frame.render_widget(p, area);
    }
}
