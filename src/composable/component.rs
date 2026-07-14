use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use serde::{Deserialize, Serialize};

use super::node::{ComponentId, ComponentNode};
use super::scroll::ScrollConfig;
use crate::reactive::DirtySignal;
use crate::runtime::{CallbackRegistry, ComponentSpec, TreeError, TreeOp};
use crate::theme::Theme;
use crate::wm::WindowId;
use crate::{ComponentCommand, ComponentError, ComponentValue};

use super::drag::{DragContext, DragOffer, DragSource, DropFeedback};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    pub mouse_coordinate_space: MouseCoordinateSpace,
    pub drag: Option<DragContext<'a>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseCoordinateSpace {
    /// Mouse coordinates are absolute terminal coordinates.
    #[default]
    Absolute,
    /// Mouse coordinates are already local to the component receiving the event.
    Local,
}

impl MouseCoordinateSpace {
    pub const fn for_child(self) -> Self {
        MouseCoordinateSpace::Local
    }
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentAction {
    #[default]
    None,
    CloseWindow,
    Changed,
    Submitted,
    ActivateMenu,
    ToggleWindowManagement,
    ToggleMaximizeWindow,
}

impl ComponentAction {
    pub(crate) const fn is_shell_command(self) -> bool {
        match self {
            Self::None | Self::CloseWindow | Self::Changed | Self::Submitted => false,
            Self::ActivateMenu | Self::ToggleWindowManagement | Self::ToggleMaximizeWindow => true,
        }
    }
}

/// Pointer-capture request a component can attach to its `EventResult`.
///
/// A component that wants to keep receiving mouse move/drag/up after the pointer
/// leaves its bounds (e.g. a button tracking a press) returns `Request` on mouse
/// down and `Release` on mouse up. The request bubbles up the container chain and
/// the window manager routes subsequent mouse events straight back to the
/// capturing component until it releases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capture {
    #[default]
    None,
    Request,
    Release,
}

/// Implements default component subtraits for a concrete component type.
///
/// Use this when a component only needs `Component::draw` and core hooks for the listed
/// responsibilities. The macro also installs the default no-op drag/drop hooks; components with
/// custom behavior should implement the corresponding subtrait explicitly instead of listing it here.
#[macro_export]
macro_rules! impl_component_default_traits {
    ($ty:ty => $($trait_name:ident),+ $(,)?) => {
        $(impl $crate::composable::$trait_name for $ty {})+
        impl $crate::composable::DragAndDrop for $ty {}
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventResult {
    pub outcome: EventOutcome,
    pub action: ComponentAction,
    pub capture: Capture,
}

impl EventResult {
    pub const fn ignored() -> Self {
        Self {
            outcome: EventOutcome::Ignored,
            action: ComponentAction::None,
            capture: Capture::None,
        }
    }

    pub const fn consumed() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ComponentAction::None,
            capture: Capture::None,
        }
    }

    pub const fn close_window() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ComponentAction::CloseWindow,
            capture: Capture::None,
        }
    }

    pub const fn changed() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ComponentAction::Changed,
            capture: Capture::None,
        }
    }

    pub const fn submitted() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: ComponentAction::Submitted,
            capture: Capture::None,
        }
    }

    /// Attach a pointer-capture request/release to this result.
    pub const fn with_capture(mut self, capture: Capture) -> Self {
        self.capture = capture;
        self
    }

    pub const fn is_consumed(self) -> bool {
        matches!(self.outcome, EventOutcome::Consumed)
            || !matches!(self.action, ComponentAction::None)
    }
}

/// Layout and size negotiation hooks used by containers and windows.
pub trait Layout: Send {
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
}

/// Scroll offset, viewport and scrollbar hooks for scrollable components.
pub trait Scrollable: Send {
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
}

/// Focus traversal hooks for focusable leaves and focus-managing containers.
pub trait FocusNav: Send {
    fn focused_child(&self) -> Option<ComponentId> {
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
}

/// Child tree and dynamic runtime update hooks.
pub trait DynamicTree: Send {
    fn tag(&self) -> Option<&str> {
        None
    }

    fn children(&self) -> &[ComponentNode] {
        &[]
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        None
    }

    fn apply_tree_ops(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        let Some(children) = self.children_mut() else {
            return Err(TreeError::InvalidTreeOp(
                "component does not support tree operations".to_string(),
            ));
        };
        let Some(index) = find_dynamic_child_index(children)? else {
            return Err(TreeError::InvalidTreeOp(
                "component does not support tree operations".to_string(),
            ));
        };
        children[index].view.apply_tree_ops(ops)
    }

    fn rebuild_tree(&mut self) -> Result<(), TreeError> {
        let Some(children) = self.children_mut() else {
            return Err(TreeError::InvalidTreeOp(
                "component does not support tree operations".to_string(),
            ));
        };
        let Some(index) = find_dynamic_child_index(children)? else {
            return Err(TreeError::InvalidTreeOp(
                "component does not support tree operations".to_string(),
            ));
        };
        children[index].view.rebuild_tree()
    }

    fn dynamic_root_spec(&self) -> Option<&ComponentSpec> {
        self.children()
            .iter()
            .find_map(|child| child.view.dynamic_root_spec())
    }

    fn dynamic_callbacks(&self) -> Option<&CallbackRegistry> {
        self.children()
            .iter()
            .find_map(|child| child.view.dynamic_callbacks())
    }
}

/// Event capture, target and bubble hooks for component event dispatch.
///
/// The desktop and window manager enter a component tree through [`EventHandling::handle_event`].
/// They do not call capture or bubble hooks around the root view. A container implementation of
/// `handle_event` is responsible for its own three-phase dispatch:
///
/// 1. Run `handle_event_capture` on itself and stop if it consumes the event.
/// 2. Dispatch to the target child for mouse events, or the focused child for keyboard/paste events,
///    by calling that child component's `handle_event`.
/// 3. If the target did not consume the event, run `handle_event_bubble` on itself as a fallback.
///
/// This makes the effective order for nested containers `outer capture -> inner capture -> target
/// handle -> inner bubble -> outer bubble` for unconsumed target events. Transparent wrappers should
/// delegate `handle_event` directly to their inner component instead of separately calling the inner
/// capture and bubble hooks around it, otherwise the inner subtree would be dispatched twice.
pub trait EventHandling: Send {
    /// Pre-target hook for container-level interception such as focus traversal.
    fn handle_event_capture(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    /// Post-target fallback hook for unconsumed events, commonly used by scrollable containers.
    fn handle_event_bubble(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    /// Main event entrypoint. Containers should orchestrate capture, target dispatch and bubble.
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }
}

/// Drag/drop hooks used by the window manager to coordinate cross-component drags.
pub trait DragAndDrop: Send {
    /// Returns a drag source for a pointer position, or `None` when dragging should not start.
    fn drag_source_at(
        &self,
        _screen_x: u16,
        _screen_y: u16,
        _ctx: ComponentContext<'_>,
    ) -> Option<DragSource> {
        None
    }

    /// Reports target feedback for the current drag offer.
    fn drag_over(&mut self, _offer: DragOffer<'_>, _ctx: ComponentContext<'_>) -> DropFeedback {
        DropFeedback::default()
    }

    /// Applies a drop offer to this component.
    fn drop(&mut self, _offer: DragOffer<'_>, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    /// Notifies a source component that its drag was cancelled or rejected.
    fn drag_cancelled(&mut self, _ctx: ComponentContext<'_>) {}
}

pub trait Component:
    Layout + Scrollable + FocusNav + DynamicTree + EventHandling + DragAndDrop + Send
{
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn is_tab_container(&self) -> bool {
        false
    }

    fn property_names(&self) -> Vec<&'static str> {
        Vec::new()
    }

    fn get_property(&self, _name: &str) -> Option<ComponentValue> {
        None
    }

    fn set_property(&mut self, name: &str, _value: ComponentValue) -> Result<(), ComponentError> {
        Err(ComponentError::unsupported_property(name))
    }

    fn dirty_signals(&self) -> Vec<DirtySignal> {
        Vec::new()
    }

    fn supports_command(&self, _command: &ComponentCommand) -> bool {
        false
    }

    fn apply_command(&mut self, _command: ComponentCommand) -> EventResult {
        EventResult::ignored()
    }

    fn titlebar(&mut self, _ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        None
    }

    fn handle_titlebar_event(&mut self, _event: &Event, _ctx: TitleBarContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>);
}

impl Layout for Box<dyn Component> {
    fn min_width(&self) -> u16 {
        self.as_ref().min_width()
    }

    fn min_height(&self) -> u16 {
        self.as_ref().min_height()
    }

    fn min_size(&self) -> (u16, u16) {
        self.as_ref().min_size()
    }

    fn desired_width(&self) -> Option<u16> {
        self.as_ref().desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.as_ref().desired_height()
    }
}

impl Scrollable for Box<dyn Component> {
    fn is_scrollable(&self) -> bool {
        self.as_ref().is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.as_ref().content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.as_ref().scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.as_ref().viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.as_ref().scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.as_mut().set_scroll_offset(x, y);
    }

    fn scroll_to(&mut self, x: u16, y: u16) {
        self.as_mut().scroll_to(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        self.as_mut().scroll_to_child(child_id);
    }
}

impl FocusNav for Box<dyn Component> {
    fn focused_child(&self) -> Option<ComponentId> {
        self.as_ref().focused_child()
    }

    fn is_focusable(&self) -> bool {
        self.as_ref().is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.as_mut().focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.as_mut().focus_last()
    }
}

impl DynamicTree for Box<dyn Component> {
    fn tag(&self) -> Option<&str> {
        self.as_ref().tag()
    }

    fn children(&self) -> &[ComponentNode] {
        self.as_ref().children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.as_mut().children_mut()
    }

    fn apply_tree_ops(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        self.as_mut().apply_tree_ops(ops)
    }

    fn rebuild_tree(&mut self) -> Result<(), TreeError> {
        self.as_mut().rebuild_tree()
    }

    fn dynamic_root_spec(&self) -> Option<&ComponentSpec> {
        self.as_ref().dynamic_root_spec()
    }

    fn dynamic_callbacks(&self) -> Option<&CallbackRegistry> {
        self.as_ref().dynamic_callbacks()
    }
}

impl EventHandling for Box<dyn Component> {
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.as_mut().handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.as_mut().handle_event_bubble(event, ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.as_mut().handle_event(event, ctx)
    }
}

impl DragAndDrop for Box<dyn Component> {
    fn drag_source_at(
        &self,
        screen_x: u16,
        screen_y: u16,
        ctx: ComponentContext<'_>,
    ) -> Option<DragSource> {
        self.as_ref().drag_source_at(screen_x, screen_y, ctx)
    }

    fn drag_over(&mut self, offer: DragOffer<'_>, ctx: ComponentContext<'_>) -> DropFeedback {
        self.as_mut().drag_over(offer, ctx)
    }

    fn drop(&mut self, offer: DragOffer<'_>, ctx: ComponentContext<'_>) -> EventResult {
        DragAndDrop::drop(self.as_mut(), offer, ctx)
    }

    fn drag_cancelled(&mut self, ctx: ComponentContext<'_>) {
        self.as_mut().drag_cancelled(ctx);
    }
}

impl Component for Box<dyn Component> {
    fn type_name(&self) -> &'static str {
        self.as_ref().type_name()
    }

    fn is_tab_container(&self) -> bool {
        self.as_ref().is_tab_container()
    }

    fn property_names(&self) -> Vec<&'static str> {
        self.as_ref().property_names()
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        self.as_ref().get_property(name)
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        self.as_mut().set_property(name, value)
    }

    fn dirty_signals(&self) -> Vec<DirtySignal> {
        self.as_ref().dirty_signals()
    }

    fn supports_command(&self, command: &ComponentCommand) -> bool {
        self.as_ref().supports_command(command)
    }

    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        self.as_mut().apply_command(command)
    }

    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.as_mut().titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        self.as_mut().handle_titlebar_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.as_mut().draw(frame, area, ctx);
    }
}

fn find_dynamic_child_index(children: &[ComponentNode]) -> Result<Option<usize>, TreeError> {
    let mut index = None;
    for (idx, child) in children.iter().enumerate() {
        if child.view.dynamic_root_spec().is_some() {
            if index.is_some() {
                return Err(TreeError::InvalidTreeOp(
                    "multiple dynamic roots found".to_string(),
                ));
            }
            index = Some(idx);
        }
    }
    Ok(index)
}
