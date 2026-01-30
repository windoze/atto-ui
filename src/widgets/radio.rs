use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct RadioGroup {
    label: String,
    options: Vec<String>,
    selected: usize,
    focused: bool,
}

impl RadioGroup {
    pub fn new(label: impl Into<String>, options: Vec<String>, selected: usize) -> Self {
        let options_len = options.len();
        Self {
            label: label.into(),
            options,
            selected: selected.min(options_len.saturating_sub(1)),
            focused: false,
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }
}

impl Control for RadioGroup {
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        let Event::Key(KeyEvent { code, .. }) = event else {
            return (ControlOutcome::Ignored, FormAction::None);
        };
        if self.options.is_empty() {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        match code {
            KeyCode::Up => {
                if self.selected == 0 {
                    self.selected = self.options.len() - 1;
                } else {
                    self.selected -= 1;
                }
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            KeyCode::Down => {
                self.selected = (self.selected + 1) % self.options.len();
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn desired_height(&self) -> u16 {
        (self.options.len() as u16).saturating_add(1)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let title_style = if self.focused {
            theme.widget.accent
        } else {
            theme.widget.dim
        };
        frame.render_widget(
            Paragraph::new(Line::styled(self.label.clone(), title_style)),
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1.min(area.height),
            },
        );
        let mut y = area.y.saturating_add(1);
        for (idx, opt) in self.options.iter().enumerate() {
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            let is_sel = idx == self.selected;
            let mark = if is_sel { "*" } else { " " };
            let style: Style = if self.focused && is_sel {
                theme.widget.focused
            } else if is_sel {
                theme.widget.accent
            } else {
                theme.widget.normal
            };
            frame.render_widget(
                Paragraph::new(Line::styled(format!("({mark}) {opt}"), style)),
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
            );
            y = y.saturating_add(1);
        }
    }
}
