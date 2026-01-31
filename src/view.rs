use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::theme::Theme;
use crate::views::{ViewId, ViewNode};
use crate::wm::WindowId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventOutcome {
    Consumed,
    #[default]
    Ignored,
}

#[derive(Clone, Copy, Debug)]
pub struct ViewContext<'a> {
    pub theme: &'a Theme,
    pub window_id: WindowId,
    pub is_focused: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewAction {
    #[default]
    None,
    CloseWindow,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewEventResult {
    pub outcome: EventOutcome,
    pub action: ViewAction,
}

impl ViewEventResult {
    pub const fn ignored() -> Self {
        Self {
            outcome: EventOutcome::Ignored,
            action: ViewAction::None,
        }
    }

    pub const fn consumed() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ViewAction::None,
        }
    }

    pub const fn close_window() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ViewAction::CloseWindow,
        }
    }

    pub const fn is_consumed(self) -> bool {
        matches!(self.outcome, EventOutcome::Consumed) || !matches!(self.action, ViewAction::None)
    }
}

pub trait View: Send {
    fn is_focusable(&self) -> bool {
        false
    }

    fn desired_width(&self) -> Option<u16> {
        None
    }

    fn desired_height(&self) -> Option<u16> {
        None
    }

    fn children(&self) -> &[ViewNode] {
        &[]
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ViewNode>> {
        None
    }

    fn handle_event_capture(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        ViewEventResult::ignored()
    }

    fn handle_event_bubble(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        ViewEventResult::ignored()
    }

    fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        ViewEventResult::ignored()
    }

    /// Returns whether this view supports scroll offsets and scrollbars.
    ///
    /// Default: `false` (non-scrollable, overflow is clipped by the parent).
    fn is_scrollable(&self) -> bool {
        false
    }

    /// Total content size of the view (may be larger than its viewport).
    ///
    /// Default: `(0, 0)` (unknown / not scrollable).
    fn content_size(&self) -> (u16, u16) {
        (0, 0)
    }

    /// Current scroll offset into the content (top-left of the viewport).
    ///
    /// Default: `(0, 0)`.
    fn scroll_offset(&self) -> (u16, u16) {
        (0, 0)
    }

    /// Sets the scroll offset. Scrollable views should clamp to a valid range.
    ///
    /// Default: no-op.
    fn set_scroll_offset(&mut self, _x: u16, _y: u16) {}

    /// Convenience method to scroll directly to an offset.
    fn scroll_to(&mut self, x: u16, y: u16) {
        self.set_scroll_offset(x, y);
    }

    /// Scrolls so the given child view is visible (and ideally centered), if supported.
    ///
    /// Default: no-op.
    fn scroll_to_child(&mut self, _child_id: ViewId) {}

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>);
}
