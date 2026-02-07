use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use atto_ui_macros::{Automatable, automate_component};
use crate::automation::AutomationAction;
use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;

#[derive(Clone, Debug, Automatable)]
pub struct RadioGroup {
    label: Binding<String>,
    options: Binding<Vec<String>>,
    #[automation(rename = "selection")]
    binding: Binding<usize>,
    enabled: Binding<bool>,
    height: Option<Binding<u16>>,
    last_area: Option<Rect>,
}

impl RadioGroup {
    pub fn new(
        label: impl Into<Binding<String>>,
        options: impl Into<Binding<Vec<String>>>,
        binding: Binding<usize>,
    ) -> Self {
        let options = options.into();
        let options_len = options.get().len();
        if options_len > 0 {
            let selected = binding.get().min(options_len.saturating_sub(1));
            binding.set(selected);
        }
        Self {
            label: label.into(),
            options,
            binding,
            enabled: true.into(),
            height: None,
            last_area: None,
        }
    }

    pub fn label(mut self, label: impl Into<Binding<String>>) -> Self {
        self.label = label.into();
        self
    }

    pub fn options(mut self, options: impl Into<Binding<Vec<String>>>) -> Self {
        self.options = options.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn height(mut self, height: impl Into<Binding<u16>>) -> Self {
        self.height = Some(height.into());
        self
    }

    pub fn selected_index(&self) -> usize {
        self.binding.get()
    }
}

#[automate_component]
impl Component for RadioGroup {
    fn automation_action(&mut self, action: AutomationAction) -> EventResult {
        match action {
            AutomationAction::SelectIndex(idx) => {
                let options_len = self.options.get().len();
                if options_len > 0 {
                    self.binding.set(idx.min(options_len.saturating_sub(1)));
                    EventResult::changed()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
        }
    }

    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        // Title row + at least one option row (if any options exist).
        if self.options.get().is_empty() { 1 } else { 2 }
    }

    fn is_focusable(&self) -> bool {
        self.enabled.get() && !self.options.get().is_empty()
    }

    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        let options = self.options.get();
        if options.is_empty() {
            return EventResult::ignored();
        }
        let mut selected = self.binding.get().min(options.len().saturating_sub(1));
        self.binding.set(selected);
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return EventResult::ignored();
                }
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };

                let options_y = area.y.saturating_add(1);
                if m.row < options_y || m.row >= options_y.saturating_add(options.len() as u16) {
                    return EventResult::ignored();
                }
                let idx = m.row.saturating_sub(options_y) as usize;
                if idx < options.len() {
                    selected = idx;
                    self.binding.set(selected);
                    return EventResult::changed();
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent { code, .. }) => {
                let len = options.len();
                match code {
                    KeyCode::Up => {
                        selected = if selected == 0 { len - 1 } else { selected - 1 };
                        self.binding.set(selected);
                        EventResult::changed()
                    }
                    KeyCode::Down => {
                        selected = (selected + 1) % len;
                        self.binding.set(selected);
                        EventResult::changed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            _ => EventResult::ignored(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        let options_len = self.options.get().len() as u16;
        let min_height = if options_len == 0 { 1 } else { 2 };
        let auto_height = options_len.saturating_add(1);
        let desired_height = self
            .height
            .as_ref()
            .map(|height| height.get())
            .unwrap_or(auto_height);

        Some(desired_height.max(min_height))
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        let enabled = self.enabled.get();
        let options = self.options.get();
        let title_style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.accent
        } else {
            ctx.theme.widget.dim
        };
        frame.render_widget(
            Paragraph::new(Line::styled(self.label.get(), title_style)),
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1.min(area.height),
            },
        );
        let mut y = area.y.saturating_add(1);
        if options.is_empty() {
            return;
        }
        let selected = self.binding.get().min(options.len().saturating_sub(1));
        for (idx, opt) in options.iter().enumerate() {
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            let is_sel = idx == selected;
            let mark = if is_sel {
                ctx.theme.glyph("radio-selected").unwrap_or("(*)")
            } else {
                ctx.theme.glyph("radio-unselected").unwrap_or("( )")
            };
            let style: Style = if !enabled {
                ctx.theme.widget.disabled
            } else if ctx.is_focused && is_sel {
                ctx.theme.widget.focused
            } else if is_sel {
                ctx.theme.widget.accent
            } else {
                ctx.theme.widget.normal
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_height_fits_all_options() {
        let selected = Binding::new(0usize);
        let radio = RadioGroup::new(
            "Mode",
            vec!["Normal".into(), "Insert".into(), "Visual".into()],
            selected,
        );
        assert_eq!(radio.desired_height(), Some(4));
    }

    #[test]
    fn explicit_height_overrides_default() {
        let selected = Binding::new(0usize);
        let radio = RadioGroup::new(
            "Mode",
            vec!["Normal".into(), "Insert".into(), "Visual".into()],
            selected,
        )
        .height(2u16);
        assert_eq!(radio.desired_height(), Some(2));
    }

    #[test]
    fn explicit_height_is_clamped_to_min_height() {
        let selected = Binding::new(0usize);
        let radio = RadioGroup::new(
            "Mode",
            vec!["Normal".into(), "Insert".into(), "Visual".into()],
            selected,
        )
        .height(1u16);
        assert_eq!(radio.desired_height(), Some(2));
    }
}
