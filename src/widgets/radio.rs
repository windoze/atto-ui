use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::ComponentCommand;
use crate::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout,
};
use crate::reactive::Binding;
use crate::runtime::{CallbackHandle, ComponentValue};
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::mouse_coords_local_to_area;

#[derive(Clone, Debug, ComponentProperties)]
pub struct RadioGroup {
    label: Binding<String>,
    options: Binding<Vec<String>>,
    #[component(rename = "selection")]
    binding: Binding<usize>,
    enabled: Binding<bool>,
    height: Option<Binding<u16>>,
    last_area: Option<Rect>,
    on_change_callback: Option<CallbackHandle>,
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
            on_change_callback: None,
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

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_change_callback = Some(callback);
        self
    }

    fn emit_change(&self) {
        if let Some(cb) = &self.on_change_callback {
            cb.emit_with(Some(ComponentValue::U64(self.binding.get() as u64)));
        }
    }
}

#[component_properties]
impl Component for RadioGroup {
    fn supports_command(&self, command: &ComponentCommand) -> bool {
        matches!(command, ComponentCommand::SelectIndex(_))
    }

    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        match command {
            ComponentCommand::SelectIndex(idx) => {
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
                ctx.theme.glyph("radio-selected").unwrap_or("(•)")
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

impl Layout for RadioGroup {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        // Title row + at least one option row (if any options exist).
        if self.options.get().is_empty() { 1 } else { 2 }
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
}

impl FocusNav for RadioGroup {
    fn is_focusable(&self) -> bool {
        self.enabled.get() && !self.options.get().is_empty()
    }
}

impl EventHandling for RadioGroup {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
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
                let Some((_local_x, local_y)) =
                    mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
                else {
                    return EventResult::ignored();
                };

                if local_y == 0 || local_y > options.len() as u16 {
                    return EventResult::ignored();
                }
                let idx = local_y.saturating_sub(1) as usize;
                if idx < options.len() {
                    selected = idx;
                    self.binding.set(selected);
                    self.emit_change();
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
                        self.emit_change();
                        EventResult::changed()
                    }
                    KeyCode::Down => {
                        selected = (selected + 1) % len;
                        self.binding.set(selected);
                        self.emit_change();
                        EventResult::changed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            _ => EventResult::ignored(),
        }
    }
}

crate::impl_component_default_traits!(RadioGroup => Scrollable, DynamicTree);

#[cfg(test)]
mod tests {
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::composable::{MouseCoordinateSpace, ScrollbarHost, TabMode};
    use crate::theme::Theme;
    use crate::wm::WindowId;

    use super::*;

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

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

    #[test]
    fn keyboard_wraps_and_mouse_hit_requires_area_contains() {
        let selected = Binding::new(0usize);
        let mut radio = RadioGroup::new(
            "Mode",
            vec!["Normal".into(), "Insert".into(), "Visual".into()],
            selected.clone(),
        );
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).expect("terminal");
        let area = Rect::new(2, 1, 16, 4);

        terminal
            .draw(|f| radio.draw(f, area, context(&theme)))
            .expect("draw");

        assert_eq!(
            radio.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
                context(&theme)
            ),
            EventResult::changed()
        );
        assert_eq!(selected.get(), 2);

        let outside = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x.saturating_sub(1),
            row: area.y + 2,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            radio.handle_event(&outside, context(&theme)),
            EventResult::ignored()
        );
        assert_eq!(selected.get(), 2);

        let inside = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 1,
            row: area.y + 2,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            radio.handle_event(&inside, context(&theme)),
            EventResult::changed()
        );
        assert_eq!(selected.get(), 1);
    }

    #[test]
    fn selected_option_uses_turbo_vision_dot_glyph() {
        let selected = Binding::new(1usize);
        let mut radio = RadioGroup::new(
            "Mode",
            vec!["Normal".into(), "Insert".into(), "Visual".into()],
            selected,
        );
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).expect("terminal");

        terminal
            .draw(|f| radio.draw(f, Rect::new(0, 0, 16, 4), context(&theme)))
            .expect("draw");

        let buf = terminal.backend().buffer();
        assert_eq!(buf[(0, 2)].symbol(), "(");
        assert_eq!(buf[(1, 2)].symbol(), "•");
        assert_eq!(buf[(2, 2)].symbol(), ")");
    }
}
