use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout,
};
use crate::reactive::Binding;
use crate::runtime::{CallbackHandle, ComponentValue};
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::{mouse_coords_local_to_area, widget_style};

#[derive(Clone, Debug, ComponentProperties)]
pub struct Checkbox {
    label: Binding<String>,
    #[component(rename = "checked")]
    binding: Binding<bool>,
    enabled: Binding<bool>,
    on_change_callback: Option<CallbackHandle>,
    last_area: Option<Rect>,
}

impl Checkbox {
    pub fn new(label: impl Into<Binding<String>>, binding: Binding<bool>) -> Self {
        Self {
            label: label.into(),
            binding,
            enabled: true.into(),
            on_change_callback: None,
            last_area: None,
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

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_change_callback = Some(callback);
        self
    }

    fn emit_change(&self) {
        if let Some(cb) = &self.on_change_callback {
            cb.emit_with(Some(ComponentValue::Bool(self.binding.get())));
        }
    }
}

#[component_properties]
impl Component for Checkbox {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        let enabled = self.enabled.get();
        let style = widget_style(ctx.theme, enabled, ctx.is_focused);
        let mark = if self.binding.get() {
            ctx.theme.glyph("checkbox-checked").unwrap_or("[x]")
        } else {
            ctx.theme.glyph("checkbox-unchecked").unwrap_or("[ ]")
        };
        let text = format!("{mark} {}", self.label.get());
        frame.render_widget(Paragraph::new(Line::styled(text, style)), area);
    }
}

impl Layout for Checkbox {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }
}

impl FocusNav for Checkbox {
    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }
}

impl EventHandling for Checkbox {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    let Some(area) = self.last_area else {
                        return EventResult::ignored();
                    };
                    if mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space).is_none() {
                        return EventResult::ignored();
                    }
                    self.binding.update(|v| *v = !*v);
                    self.emit_change();
                    return EventResult::changed();
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char(' ') | KeyCode::Enter,
                ..
            }) => {
                self.binding.update(|v| *v = !*v);
                self.emit_change();
                EventResult::changed()
            }
            Event::Key(KeyEvent { .. }) => EventResult::ignored(),
            _ => EventResult::ignored(),
        }
    }
}

crate::impl_component_default_traits!(Checkbox => Scrollable, DynamicTree);

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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
        }
    }

    #[test]
    fn mouse_down_requires_last_area_hit() {
        let checked = Binding::new(false);
        let mut checkbox = Checkbox::new("Enabled", checked.clone());
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(20, 6)).expect("terminal");
        terminal
            .draw(|f| checkbox.draw(f, Rect::new(4, 2, 12, 1), context(&theme)))
            .expect("draw");

        let outside = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 2,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            checkbox.handle_event(&outside, context(&theme)),
            EventResult::ignored()
        );
        assert!(!checked.get());

        let inside = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 2,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            checkbox.handle_event(&inside, context(&theme)),
            EventResult::changed()
        );
        assert!(checked.get());
    }
}
