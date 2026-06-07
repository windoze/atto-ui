use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout, MouseCoordinateSpace,
};
use crate::reactive::Binding;
use crate::runtime::{CallbackHandle, ComponentValue};
use atto_ui_macros::{ComponentProperties, component_properties};

#[derive(Clone, Debug, ComponentProperties)]
pub struct Slider {
    min: Binding<f64>,
    max: Binding<f64>,
    value: Binding<f64>,
    step: Binding<f64>,
    enabled: Binding<bool>,
    fill_char: char,
    empty_char: char,
    thumb_char: char,
    last_area: Option<Rect>,
    on_change_callback: Option<CallbackHandle>,
}

impl Slider {
    pub fn new(
        min: impl Into<Binding<f64>>,
        max: impl Into<Binding<f64>>,
        value: Binding<f64>,
    ) -> Self {
        Self {
            min: min.into(),
            max: max.into(),
            value,
            step: 1.0.into(),
            enabled: true.into(),
            fill_char: '=',
            empty_char: '-',
            thumb_char: '|',
            last_area: None,
            on_change_callback: None,
        }
    }

    pub fn min(mut self, min: impl Into<Binding<f64>>) -> Self {
        self.min = min.into();
        self
    }

    pub fn max(mut self, max: impl Into<Binding<f64>>) -> Self {
        self.max = max.into();
        self
    }

    pub fn value(mut self, value: Binding<f64>) -> Self {
        self.value = value;
        self
    }

    pub fn step(mut self, step: impl Into<Binding<f64>>) -> Self {
        self.step = step.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn fill_char(mut self, ch: char) -> Self {
        self.fill_char = ch;
        self
    }

    pub fn empty_char(mut self, ch: char) -> Self {
        self.empty_char = ch;
        self
    }

    pub fn thumb_char(mut self, ch: char) -> Self {
        self.thumb_char = ch;
        self
    }

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_change_callback = Some(callback);
        self
    }

    fn normalized_range(&self) -> (f64, f64) {
        let mut min = self.min.get();
        let mut max = self.max.get();
        if max < min {
            std::mem::swap(&mut min, &mut max);
        }
        (min, max)
    }

    fn clamp_value(&self, value: f64) -> f64 {
        let (min, max) = self.normalized_range();
        if value < min {
            min
        } else if value > max {
            max
        } else {
            value
        }
    }

    fn snap_value(&self, value: f64) -> f64 {
        let (min, max) = self.normalized_range();
        let step = self.step.get().abs();
        if step <= f64::EPSILON {
            return self.clamp_value(value);
        }
        let steps = ((value - min) / step).round();
        let snapped = min + steps * step;
        if snapped < min {
            min
        } else if snapped > max {
            max
        } else {
            snapped
        }
    }

    fn value_from_pos(&self, x: u16, width: u16) -> f64 {
        let (min, max) = self.normalized_range();
        let range = max - min;
        if width <= 1 || range.abs() <= f64::EPSILON {
            return min;
        }
        let ratio = (x as f64) / (width.saturating_sub(1) as f64);
        let value = min + ratio * range;
        self.snap_value(value)
    }

    fn thumb_pos(&self, width: u16) -> u16 {
        let (min, max) = self.normalized_range();
        let range = max - min;
        if width <= 1 || range.abs() <= f64::EPSILON {
            return 0;
        }
        let value = self.clamp_value(self.value.get());
        let ratio = (value - min) / range;
        let pos = (ratio * (width.saturating_sub(1) as f64)).round() as u16;
        pos.min(width.saturating_sub(1))
    }

    fn set_value_from_mouse(
        &mut self,
        area: Rect,
        m: MouseEvent,
        coordinate_space: MouseCoordinateSpace,
    ) -> EventResult {
        let Some((local_x, _local_y)) = mouse_coords_local_to_area(area, m, coordinate_space)
        else {
            return EventResult::ignored();
        };
        let value = self.value_from_pos(local_x, area.width);
        self.set_value_and_emit(value)
    }

    fn adjust_by_step(&mut self, delta: f64) -> EventResult {
        let value = self.value.get();
        let next = self.snap_value(value + delta);
        self.set_value_and_emit(next)
    }

    fn set_value_and_emit(&mut self, value: f64) -> EventResult {
        let prev = self.value.get();
        self.value.set(value);
        if (prev - value).abs() > f64::EPSILON
            && let Some(cb) = &self.on_change_callback
        {
            cb.emit_with(Some(ComponentValue::F64(value)));
        }
        EventResult::changed()
    }
}

#[component_properties]
impl Component for Slider {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let enabled = self.enabled.get();
        let fill_style = if enabled {
            ctx.theme.widget.accent
        } else {
            ctx.theme.widget.disabled
        };
        let empty_style = if enabled {
            ctx.theme.widget.dim
        } else {
            ctx.theme.widget.disabled
        };
        let thumb_style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };

        let width = area.width as usize;
        let thumb_pos = self.thumb_pos(area.width) as usize;
        let mut spans = Vec::with_capacity(width);
        for idx in 0..width {
            let (ch, style) = if idx == thumb_pos {
                (self.thumb_char, thumb_style)
            } else if idx < thumb_pos {
                (self.fill_char, fill_style)
            } else {
                (self.empty_char, empty_style)
            };
            spans.push(Span::styled(ch.to_string(), style));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

impl Layout for Slider {
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

impl FocusNav for Slider {
    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }
}

impl EventHandling for Slider {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }

        match event {
            Event::Mouse(m) => {
                if matches!(m.kind, MouseEventKind::Down(MouseButton::Left))
                    || matches!(m.kind, MouseEventKind::Drag(MouseButton::Left))
                {
                    let Some(area) = self.last_area else {
                        return EventResult::ignored();
                    };
                    return self.set_value_from_mouse(area, *m, ctx.mouse_coordinate_space);
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Left => self.adjust_by_step(-self.step.get().abs()),
                KeyCode::Right => self.adjust_by_step(self.step.get().abs()),
                KeyCode::Home => {
                    let (min, _max) = self.normalized_range();
                    self.set_value_and_emit(min)
                }
                KeyCode::End => {
                    let (_min, max) = self.normalized_range();
                    self.set_value_and_emit(max)
                }
                _ => EventResult::ignored(),
            },
            _ => EventResult::ignored(),
        }
    }
}

crate::impl_component_default_traits!(Slider => Scrollable, DynamicTree);

fn mouse_coords_local_to_area(
    area: Rect,
    m: MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    match coordinate_space {
        MouseCoordinateSpace::Absolute => (area.width > 0
            && area.height > 0
            && m.column >= area.x
            && m.column < area.x.saturating_add(area.width)
            && m.row >= area.y
            && m.row < area.y.saturating_add(area.height))
        .then(|| {
            (
                m.column.saturating_sub(area.x),
                m.row.saturating_sub(area.y),
            )
        }),
        MouseCoordinateSpace::Local => {
            (area.width > 0 && area.height > 0 && m.column < area.width && m.row < area.height)
                .then_some((m.column, m.row))
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseEvent};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::composable::{ScrollbarHost, TabMode};
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
    fn keyboard_mouse_and_disabled_states_update_value_safely() {
        let value = Binding::new(5.0);
        let mut slider = Slider::new(0.0, 10.0, value.clone()).step(2.0);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(24, 4)).expect("terminal");
        let area = Rect::new(2, 1, 11, 1);
        terminal
            .draw(|f| slider.draw(f, area, context(&theme)))
            .expect("draw");

        let right = Event::Key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(
            slider.handle_event(&right, context(&theme)),
            EventResult::changed()
        );
        assert_eq!(value.get(), 8.0);

        let end = Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        assert_eq!(
            slider.handle_event(&end, context(&theme)),
            EventResult::changed()
        );
        assert_eq!(value.get(), 10.0);

        let outside = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x.saturating_sub(1),
            row: area.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            slider.handle_event(&outside, context(&theme)),
            EventResult::ignored()
        );
        assert_eq!(value.get(), 10.0);

        let inside = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 5,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            slider.handle_event(&inside, context(&theme)),
            EventResult::changed()
        );
        assert_eq!(value.get(), 6.0);

        let mut disabled = slider.clone().enabled(false);
        assert_eq!(
            disabled.handle_event(&right, context(&theme)),
            EventResult::ignored()
        );
        assert_eq!(value.get(), 6.0);
    }
}
