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
    enabled: bool,
    area: Option<Rect>,
}

impl RadioGroup {
    pub fn new(label: impl Into<String>, options: Vec<String>, selected: usize) -> Self {
        let options_len = options.len();
        Self {
            label: label.into(),
            options,
            selected: selected.min(options_len.saturating_sub(1)),
            focused: false,
            enabled: true,
            area: None,
        }
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }
}

impl Control for RadioGroup {
    fn is_focusable(&self) -> bool {
        self.enabled && !self.options.is_empty()
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn set_area(&mut self, area: Rect) {
        self.area = Some(area);
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        if !self.enabled {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        if self.options.is_empty() {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return (ControlOutcome::Ignored, FormAction::None);
                }
                let Some(area) = self.area else {
                    return (ControlOutcome::Ignored, FormAction::None);
                };

                let options_y = area.y.saturating_add(1);
                if m.row < options_y || m.row >= options_y.saturating_add(self.options.len() as u16)
                {
                    return (ControlOutcome::Ignored, FormAction::None);
                }
                let idx = m.row.saturating_sub(options_y) as usize;
                if idx < self.options.len() {
                    self.selected = idx;
                    return (ControlOutcome::Consumed, FormAction::Changed);
                }
                (ControlOutcome::Ignored, FormAction::None)
            }
            Event::Key(KeyEvent { code, .. }) => match code {
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
            },
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn desired_height(&self) -> u16 {
        (self.options.len() as u16).saturating_add(1)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let title_style = if !self.enabled {
            theme.widget.disabled
        } else if self.focused {
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
            let mark = if is_sel {
                theme.glyph("radio-selected").unwrap_or("(*)")
            } else {
                theme.glyph("radio-unselected").unwrap_or("( )")
            };
            let style: Style = if !self.enabled {
                theme.widget.disabled
            } else if self.focused && is_sel {
                theme.widget.focused
            } else if is_sel {
                theme.widget.accent
            } else {
                theme.widget.normal
            };
            frame.render_widget(
                Paragraph::new(Line::styled(format!("{mark} {opt}"), style)),
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
