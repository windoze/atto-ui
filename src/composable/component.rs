use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::automation::{AutomationAction, AutomationError, AutomationValue};
use super::node::{ComponentId, ComponentNode};
use super::scroll::ScrollConfig;
use crate::theme::Theme;
use crate::wm::WindowId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventOutcome {
    Consumed,
    #[default]
    Ignored,
}

#[derive(Clone, Copy, Debug)]
pub struct ComponentContext<'a> {
    pub theme: &'a Theme,
    pub window_id: WindowId,
    pub is_focused: bool,
    pub scrollbar_host: ScrollbarHost,
    pub tab_mode: TabMode,
}

#[derive(Clone, Debug)]
pub struct TitleBarSpan {
    pub text: String,
    pub style: Option<Style>,
}

impl TitleBarSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style: Some(style),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TitleBarContent {
    pub spans: Vec<TitleBarSpan>,
}

impl TitleBarContent {
    pub fn push(&mut self, span: TitleBarSpan) {
        self.spans.push(span);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TitleBarContext<'a> {
    pub theme: &'a Theme,
    pub window_id: WindowId,
    pub is_focused: bool,
    pub area: Rect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarHost {
    /// The component should render and handle its own scrollbars.
    #[default]
    Component,
    /// Scrollbars are rendered/handled by the window chrome for the root view.
    ///
    /// Child components should treat this as [`ScrollbarHost::Component`] so nested scrollables keep working.
    Window,
}

impl ScrollbarHost {
    pub const fn for_child(self) -> Self {
        match self {
            ScrollbarHost::Component => ScrollbarHost::Component,
            ScrollbarHost::Window => ScrollbarHost::Component,
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
pub enum ComponentAction {
    #[default]
    None,
    CloseWindow,
    Changed,
    Submitted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventResult {
    pub outcome: EventOutcome,
    pub action: ComponentAction,
}

impl EventResult {
    pub const fn ignored() -> Self {
        Self {
            outcome: EventOutcome::Ignored,
            action: ComponentAction::None,
        }
    }

    pub const fn consumed() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ComponentAction::None,
        }
    }

    pub const fn close_window() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ComponentAction::CloseWindow,
        }
    }

    pub const fn changed() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ComponentAction::Changed,
        }
    }

    pub const fn submitted() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ComponentAction::Submitted,
        }
    }

    pub const fn is_consumed(self) -> bool {
        matches!(self.outcome, EventOutcome::Consumed)
            || !matches!(self.action, ComponentAction::None)
    }
}

pub trait Component: Send {
    fn automation_type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn automation_id(&self) -> Option<&str> {
        None
    }

    fn automation_properties(&self) -> Vec<&'static str> {
        Vec::new()
    }

    fn automation_get_property(&self, _name: &str) -> Option<AutomationValue> {
        None
    }

    fn automation_set_property(
        &mut self,
        name: &str,
        _value: AutomationValue,
    ) -> Result<(), AutomationError> {
        Err(AutomationError::unsupported_property(name))
    }

    fn automation_action(&mut self, _action: AutomationAction) -> EventResult {
        EventResult::ignored()
    }

    fn automation_focused_child(&self) -> Option<ComponentId> {
        None
    }

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

    fn children(&self) -> &[ComponentNode] {
        &[]
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        None
    }

    fn handle_event_capture(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn handle_event_bubble(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn titlebar(&mut self, _ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        None
    }

    fn handle_titlebar_event(&mut self, _event: &Event, _ctx: TitleBarContext<'_>) -> EventResult {
        EventResult::ignored()
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
    fn scroll_to_child(&mut self, _child_id: ComponentId) {}

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>);
}
