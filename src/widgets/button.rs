use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct Button {
    label: String,
    focused: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            focused: false,
        }
    }
}

impl Control for Button {
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        let Event::Key(KeyEvent { code, .. }) = event else {
            return (ControlOutcome::Ignored, FormAction::None);
        };
        match code {
            KeyCode::Enter | KeyCode::Char(' ') => (ControlOutcome::Consumed, FormAction::Submitted),
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn desired_height(&self) -> u16 {
        3
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let style = if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };
        let block = Block::default().borders(Borders::ALL).border_style(style);
        let text = Line::styled(format!(" {} ", self.label), style);
        let p = Paragraph::new(text).block(block);
        frame.render_widget(p, area);
    }
}

