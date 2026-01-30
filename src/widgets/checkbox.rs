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
}

impl Checkbox {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            focused: false,
        }
    }

    pub fn checked(&self) -> bool {
        self.checked
    }
}

impl Control for Checkbox {
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        let Event::Key(KeyEvent { code, .. }) = event else {
            return (ControlOutcome::Ignored, FormAction::None);
        };
        match code {
            KeyCode::Char(' ') | KeyCode::Enter => {
                self.checked = !self.checked;
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let style = if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };
        let mark = if self.checked { "x" } else { " " };
        let text = format!("[{mark}] {}", self.label);
        frame.render_widget(Paragraph::new(Line::styled(text, style)), area);
    }
}
