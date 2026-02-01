use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::reactive::Binding;
use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct Label {
    text: Binding<String>,
    enabled: Binding<bool>,
}

impl Label {
    pub fn new(text: impl Into<Binding<String>>) -> Self {
        Self {
            text: text.into(),
            enabled: true.into(),
        }
    }

    pub fn text(mut self, text: impl Into<Binding<String>>) -> Self {
        self.text = text.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
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
        let style = if self.enabled.get() {
            theme.widget.dim
        } else {
            theme.widget.disabled
        };
        let p = Paragraph::new(Line::styled(self.text.get(), style));
        frame.render_widget(p, area);
    }
}
