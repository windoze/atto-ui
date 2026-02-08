use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use atto_ui_macros::{ComponentProperties, component_properties};
use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;

#[derive(Clone, Debug, ComponentProperties)]
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

#[component_properties]
impl Component for Label {
    fn is_focusable(&self) -> bool {
        false
    }

    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        Some(self.text.get().len().min(u16::MAX as usize) as u16)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let style = if self.enabled.get() {
            ctx.theme.widget.dim
        } else {
            ctx.theme.widget.disabled
        };
        let p = Paragraph::new(Line::styled(self.text.get(), style));
        frame.render_widget(p, area);
    }
}
