use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::{View, ViewContext, ViewEventResult};
use crate::views::{ScrollConfig, ViewId, ViewNode};

use super::view::DeclarativeView;

/// Bridges a declarative root into Chatty's imperative [`View`] runtime.
///
/// This is intentionally thin: it materializes the declarative tree once via
/// [`DeclarativeView::build_view`] and forwards all behavior to the resulting imperative view.
///
/// Note: The current implementation does **not** automatically rebuild the imperative tree when
/// state changes. Reactive updates are expected to flow through bindings used by the underlying
/// widgets/views.
pub struct ViewAdapter {
    inner: Box<dyn View>,
}

impl ViewAdapter {
    pub fn new(root: impl DeclarativeView) -> Self {
        Self {
            inner: root.build_view(),
        }
    }
}

impl View for ViewAdapter {
    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }

    fn desired_width(&self) -> Option<u16> {
        self.inner.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.inner.desired_height()
    }

    fn children(&self) -> &[ViewNode] {
        self.inner.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ViewNode>> {
        self.inner.children_mut()
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        self.inner.handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        self.inner.handle_event_bubble(event, ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        self.inner.handle_event(event, ctx)
    }

    fn is_scrollable(&self) -> bool {
        self.inner.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.inner.content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.inner.scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.inner.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.inner.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.inner.set_scroll_offset(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ViewId) {
        self.inner.scroll_to_child(child_id);
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.inner.draw(frame, area, ctx);
    }
}
