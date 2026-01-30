use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

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
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    return (ControlOutcome::Consumed, FormAction::Submitted);
                }
                (ControlOutcome::Ignored, FormAction::None)
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    (ControlOutcome::Consumed, FormAction::Submitted)
                }
                _ => (ControlOutcome::Ignored, FormAction::None),
            },
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
