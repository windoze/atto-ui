use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::theme::Theme;
use crate::views::{ScrollConfig, ViewId, ViewNode};
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
    pub scrollbar_host: ScrollbarHost,
    pub tab_mode: TabMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarHost {
    /// The view should render and handle its own scrollbars.
    #[default]
    View,
    /// Scrollbars are rendered/handled by the window chrome for the root view.
    ///
    /// Child views should treat this as [`ScrollbarHost::View`] so nested scrollables keep working.
    Window,
}

impl ScrollbarHost {
    pub const fn for_child(self) -> Self {
        match self {
            ScrollbarHost::View => ScrollbarHost::View,
            ScrollbarHost::Window => ScrollbarHost::View,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabMode {
    /// Tab key navigation should "bubble" out of the current container when it reaches the end.
    ///
    /// This is the default for nested containers so focus traversal can follow the visual tree
    /// instead of getting trapped within a sub-layout.
    #[default]
    Bubble,
    /// Tab key navigation should wrap around within this focus scope.
    ///
    /// This is typically used at the window root so `Tab` always stays within the window.
    Cycle,
}

impl TabMode {
    pub const fn for_child(self) -> Self {
        TabMode::Bubble
    }
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

    /// Focus the first focusable descendant in this view subtree.
    ///
    /// Container views should override this to set their internal focused child and recurse.
    ///
    /// Returns `true` if this view (or one of its descendants) is focusable.
    fn focus_first(&mut self) -> bool {
        self.is_focusable()
    }

    /// Focus the last focusable descendant in this view subtree.
    ///
    /// Returns `true` if this view (or one of its descendants) is focusable.
    fn focus_last(&mut self) -> bool {
        self.is_focusable()
    }

    /// Minimum width required for this view to be usable.
    ///
    /// This is used by layout containers (and optionally windows) to avoid rendering focusable
    /// widgets in partially-clipped states where they cannot be interacted with reliably.
    ///
    /// Default: `0` (no minimum).
    fn min_width(&self) -> u16 {
        0
    }

    /// Minimum height required for this view to be usable.
    ///
    /// Default: `0` (no minimum).
    fn min_height(&self) -> u16 {
        0
    }

    fn min_size(&self) -> (u16, u16) {
        (self.min_width(), self.min_height())
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

    /// Visible viewport size (width, height) used for scrollbar math.
    ///
    /// Scrollable views should return the size of their visible content area (after padding).
    ///
    /// Default: `(0, 0)` (unknown / not scrollable).
    fn viewport_size(&self) -> (u16, u16) {
        (0, 0)
    }

    /// Scrollbar configuration for scrollable views.
    ///
    /// Default: `ScrollConfig::default()`.
    fn scroll_config(&self) -> ScrollConfig {
        ScrollConfig::default()
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
