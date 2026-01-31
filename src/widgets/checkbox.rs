use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct Checkbox {
    label: String,
    checked: bool,
    focused: bool,
    enabled: bool,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            focused: false,
            enabled: true,
        }
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn checked(&self) -> bool {
        self.checked
    }
}

impl Control for Checkbox {
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
                    self.checked = !self.checked;
                    return (ControlOutcome::Consumed, FormAction::Changed);
                }
                (ControlOutcome::Ignored, FormAction::None)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char(' ') | KeyCode::Enter,
                ..
            }) => {
                self.checked = !self.checked;
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            Event::Key(KeyEvent { .. }) => (ControlOutcome::Ignored, FormAction::None),
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let style = if !self.enabled {
            theme.widget.disabled
        } else if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };
        let mark = if self.checked {
            theme.glyph("checkbox-checked").unwrap_or("[x]")
        } else {
            theme.glyph("checkbox-unchecked").unwrap_or("[ ]")
        };
        let text = format!("{mark} {}", self.label);
        frame.render_widget(Paragraph::new(Line::styled(text, style)), area);
    }
}
