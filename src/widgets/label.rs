use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct Label {
    text: String,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl Control for Label {
    fn is_focusable(&self) -> bool {
        false
    }

    fn handle_event(&mut self, _event: &Event) -> (ControlOutcome, FormAction) {
        (ControlOutcome::Ignored, FormAction::None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let p = Paragraph::new(Line::styled(self.text.clone(), theme.widget.dim));
        frame.render_widget(p, area);
    }
}

