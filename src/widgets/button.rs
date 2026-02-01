use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone)]
pub struct Button {
    label: String,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    focused: bool,
    enabled: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_click: None,
            focused: false,
            enabled: true,
        }
    }

    pub fn on_click<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_click = Some(Arc::new(callback));
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn trigger(&self) {
        if let Some(cb) = &self.on_click {
            cb();
        }
    }
}

impl Control for Button {
    fn is_focusable(&self) -> bool {
        self.enabled
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        if !self.enabled {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.trigger();
                    return (ControlOutcome::Consumed, FormAction::Submitted);
                }
                (ControlOutcome::Ignored, FormAction::None)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Enter | KeyCode::Char(' '),
                ..
            }) => {
                self.trigger();
                (ControlOutcome::Consumed, FormAction::Submitted)
            }
            Event::Key(KeyEvent { .. }) => (ControlOutcome::Ignored, FormAction::None),
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn desired_height(&self) -> u16 {
        3
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let style = if !self.enabled {
            theme.widget.disabled
        } else if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .border_set(theme.border_set(false));
        let text = Line::styled(format!(" {} ", self.label), style);
        let p = Paragraph::new(text).block(block);
        frame.render_widget(p, area);
    }
}
