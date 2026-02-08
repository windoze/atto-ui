use std::sync::Arc;

use atto_ui_macros::{ComponentProperties, component_properties};
use crate::ComponentValue;
use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;
use crate::runtime::CallbackHandle;
use crate::text::styled_text::{
    hit_test_link, inline_display_width, parse_inline, spans_from_segments,
};
use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

type LinkCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// A single-line label that supports a small subset of inline markdown-like styling.
///
/// Supported syntax (markers are hidden in the rendered output):
/// - `**bold**`
/// - `*italic*`
/// - `__underline__`
/// - `~~strikethrough~~`
/// - `[link text](url)` (link text is underlined; clicking calls `on_link(url)`)
///
/// Parsing is intentionally simple (no full markdown support).
#[derive(Clone, ComponentProperties)]
pub struct StyledLabel {
    text: Binding<String>,
    enabled: Binding<bool>,
    on_link: Option<LinkCallback>,
    on_link_callback: Option<CallbackHandle>,
    last_area: Option<Rect>,
}

impl StyledLabel {
    pub fn new(text: impl Into<Binding<String>>) -> Self {
        Self {
            text: text.into(),
            enabled: true.into(),
            on_link: None,
            on_link_callback: None,
            last_area: None,
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

    pub fn on_link<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_link = Some(Arc::new(callback));
        self
    }

    pub fn on_link_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_link_callback = Some(callback);
        self
    }
}

#[component_properties]
impl Component for StyledLabel {
    fn is_focusable(&self) -> bool {
        false
    }

    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }

        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        if m.kind != MouseEventKind::Down(MouseButton::Left) {
            return EventResult::ignored();
        }

        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };
        let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
            return EventResult::ignored();
        };
        if local_y != 0 {
            return EventResult::ignored();
        }

        let segments = parse_inline(&self.text.get());
        if let Some(url) = hit_test_link(&segments, local_x) {
            if let Some(cb) = &self.on_link {
                cb(url);
            }
            if let Some(cb) = &self.on_link_callback {
                cb.emit_with(Some(ComponentValue::String(url.to_string())));
            }
            return EventResult::consumed();
        }

        EventResult::ignored()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        Some(inline_display_width(&self.text.get()))
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base = if self.enabled.get() {
            ctx.theme.widget.dim
        } else {
            ctx.theme.widget.disabled
        };
        let link_overlay = ctx.theme.named_style("markdown-link");

        let segments = parse_inline(&self.text.get());
        let spans = spans_from_segments(&segments, base, link_overlay);
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
    if area.width == 0 || area.height == 0 {
        return None;
    }

    if m.column >= area.x
        && m.column < area.x.saturating_add(area.width)
        && m.row >= area.y
        && m.row < area.y.saturating_add(area.height)
    {
        return Some((
            m.column.saturating_sub(area.x),
            m.row.saturating_sub(area.y),
        ));
    }

    // Events from nested containers are already relative to our own origin.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}
