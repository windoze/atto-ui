use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::reactive::Binding;
use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct Checkbox {
    label: Binding<String>,
    binding: Binding<bool>,
    focused: bool,
    enabled: Binding<bool>,
}

impl Checkbox {
    pub fn new(label: impl Into<Binding<String>>, binding: Binding<bool>) -> Self {
        Self {
            label: label.into(),
            binding,
            focused: false,
            enabled: true.into(),
        }
    }

    pub fn label(mut self, label: impl Into<Binding<String>>) -> Self {
        self.label = label.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }
}

impl Control for Checkbox {
    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }

    fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        if !self.enabled.get() {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.binding.update(|v| *v = !*v);
                    return (ControlOutcome::Consumed, FormAction::Changed);
                }
                (ControlOutcome::Ignored, FormAction::None)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char(' ') | KeyCode::Enter,
                ..
            }) => {
                self.binding.update(|v| *v = !*v);
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            Event::Key(KeyEvent { .. }) => (ControlOutcome::Ignored, FormAction::None),
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let enabled = self.enabled.get();
        let style = if !enabled {
            theme.widget.disabled
        } else if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };
        let mark = if self.binding.get() {
            theme.glyph("checkbox-checked").unwrap_or("[x]")
        } else {
            theme.glyph("checkbox-unchecked").unwrap_or("[ ]")
        };
        let text = format!("{mark} {}", self.label.get());
        frame.render_widget(Paragraph::new(Line::styled(text, style)), area);
    }
}
