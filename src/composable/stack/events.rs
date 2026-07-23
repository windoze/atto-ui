use crossterm::event::Event;
use ratatui::layout::Rect;

use super::super::component::{ComponentContext, EventResult, MouseCoordinateSpace};
use super::super::focus_container::{self, FocusableContainer};
use super::super::layout::EdgeInsets;
use super::super::node::{ComponentId, ComponentNode};
use super::super::scroll::{ScrollConfig, ScrollOffset, ScrollbarDrag, Scrollbars};
use super::StackCore;

impl FocusableContainer for StackCore {
    fn children(&self) -> &[ComponentNode] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<ComponentNode> {
        &mut self.children
    }
    fn focused(&self) -> Option<ComponentId> {
        self.focused
    }
    fn set_focused(&mut self, id: Option<ComponentId>) {
        self.focused = id;
    }
    fn captured_child(&self) -> Option<ComponentId> {
        self.captured_child
    }
    fn set_captured_child(&mut self, id: Option<ComponentId>) {
        self.captured_child = id;
    }
    fn last_area(&self) -> Option<Rect> {
        self.last_area
    }
    fn scrollable(&self) -> bool {
        self.scrollable.get()
    }
    fn scroll(&self) -> ScrollOffset {
        self.scroll.get()
    }
    fn set_scroll(&mut self, offset: ScrollOffset) {
        self.scroll.set(offset);
    }
    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config.get()
    }
    fn padding(&self) -> EdgeInsets {
        self.padding.get()
    }
    fn scrollbars(&self) -> Option<Scrollbars> {
        self.scrollbars
    }
    fn scrollbar_drag_mut(&mut self) -> &mut Option<ScrollbarDrag> {
        &mut self.scrollbar_drag
    }
    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }
    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }
}

impl StackCore {
    pub(super) fn first_focusable_child(&self) -> Option<ComponentId> {
        focus_container::first_focusable_child(self)
    }

    pub(super) fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        focus_container::scroll_to_clamped(self, x, y)
    }

    pub(super) fn handle_event_capture_impl(
        &mut self,
        event: &Event,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        focus_container::handle_event_capture(self, event, ctx)
    }

    pub(super) fn handle_event_bubble_impl(
        &mut self,
        event: &Event,
        coordinate_space: MouseCoordinateSpace,
    ) -> EventResult {
        focus_container::handle_event_bubble(self, event, coordinate_space)
    }

    pub(super) fn handle_event_impl(
        &mut self,
        event: &Event,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        focus_container::handle_event(self, event, ctx)
    }
}
