//! In-process introspection / scriptable control plane over a [`Desktop`].
//!
//! This module groups the inspector facade and its supporting machinery into
//! focused submodules:
//!
//! - [`mod@self::types`]: snapshot/inspect node and result value types.
//! - [`mod@self::change`]: dirty-signal collection and the change tracker.
//! - [`mod@self::tree`]: inspect-node tree and serializable snapshot tree builders.
//! - [`mod@self::snapshot`]: value-clipping helpers and buffer/geometry utilities.
//! - [`mod@self::dispatch`]: tag-based backend dispatch (menu / window / component).
//! - [`mod@self::wait`]: wait-condition polling helpers.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::Desktop;
use crate::composable::EventResult;
use crate::runtime::ComponentValue;
use crate::{ComponentCommand, ComponentError, ComponentTarget};

mod change;
mod dispatch;
mod snapshot;
mod tree;
mod types;
mod wait;

// Pull the private helpers back into the inspector scope so the facade's
// existing call sites resolve unchanged after the split.
use change::*;
use dispatch::*;
use snapshot::*;
use tree::*;
use wait::*;

pub use types::{
    DesktopChangeTracker, DesktopSnapshot, DesktopSnapshotNode, InspectNode, InspectSnapshot,
    InvokeDispatch, InvokeResult, NodeKind, WaitCondition, WaitResult,
};

/// In-process control façade over a [`Desktop`] for scripting and tests.
///
/// Despite the "inspector" name this is **not read-only**. It holds `&mut Desktop` and exposes both
/// reads (`tree`, `snapshot`, `get_property`, `query`, …) and writes (`set_property`, `action`,
/// `invoke`, `click`, `input_text`, …). Even the read methods take `&mut self` and run a layout/draw
/// pass (`draw_desktop`) so bounds and dirty state reflect the current frame — i.e. reading has
/// rendering side effects. Treat it as a mutable handle, not a pure view. (This is the shared entry
/// point for both the layer-1 introspection reads and the layer-2 scriptable writes; the two are not
/// separated at the type level.)
pub struct DesktopInspector<'a> {
    desktop: &'a mut Desktop,
}

impl<'a> DesktopInspector<'a> {
    pub fn new(desktop: &'a mut Desktop) -> Self {
        Self { desktop }
    }

    pub fn change_tracker(&self) -> DesktopChangeTracker {
        DesktopChangeTracker::new(collect_desktop_dirty_signals(self.desktop))
    }

    pub fn refresh_change_tracker(&self, tracker: &mut DesktopChangeTracker) {
        tracker.refresh(collect_desktop_dirty_signals(self.desktop));
    }

    pub fn tree(&mut self, screen: Rect) -> Result<InspectNode, ComponentError> {
        Ok(self.snapshot(screen)?.tree)
    }

    pub fn snapshot(&mut self, screen: Rect) -> Result<InspectSnapshot, ComponentError> {
        let terminal = draw_desktop(self.desktop, screen)?;
        let buffer = terminal.backend().buffer().clone();
        let tree = build_desktop_tree(self.desktop, screen);
        Ok(InspectSnapshot { buffer, tree })
    }

    pub fn export_snapshot(&mut self, screen: Rect) -> Result<DesktopSnapshot, ComponentError> {
        draw_desktop(self.desktop, screen)?;
        Ok(DesktopSnapshot {
            bounds: runtime_rect(screen),
            tree: build_desktop_snapshot_tree(self.desktop, screen),
        })
    }

    pub fn get_property(&mut self, id: &str, name: &str) -> Result<ComponentValue, ComponentError> {
        // Preserves the original chain's error: a resolved id whose backend does not expose `name`
        // falls through to `not_found(id)` (not `unsupported_property`).
        match resolve_dispatch_target(self.desktop, id) {
            Some(DispatchTarget::Menu) => menu_get_property(&self.desktop.menu, id, name),
            Some(DispatchTarget::Window) => window_get_property(&self.desktop.wm, id, name),
            Some(DispatchTarget::Component) => component_get_property(&self.desktop.wm, id, name),
            None => None,
        }
        .ok_or_else(|| ComponentError::not_found(id))
    }

    pub fn property_names(&mut self, id: &str) -> Result<Vec<String>, ComponentError> {
        match resolve_dispatch_target(self.desktop, id) {
            Some(DispatchTarget::Menu) => menu_property_names(&self.desktop.menu, id),
            Some(DispatchTarget::Window) => window_property_names(&self.desktop.wm, id),
            Some(DispatchTarget::Component) => component_property_names(&self.desktop.wm, id),
            None => None,
        }
        .ok_or_else(|| ComponentError::not_found(id))
    }

    pub fn query(
        &mut self,
        target: ComponentTarget,
        property: &str,
    ) -> Result<ComponentValue, ComponentError> {
        match target {
            ComponentTarget::Id(id) => self.get_property(&id, property),
            ComponentTarget::Focused => {
                let Some(focused) = focused_component_mut(&mut self.desktop.wm) else {
                    return Err(ComponentError::not_found("focused"));
                };
                focused
                    .get_property(property)
                    .ok_or_else(|| ComponentError::unsupported_property(property))
            }
        }
    }

    /// Returns interactive nodes that cannot be targeted by tag-based scripts.
    pub fn untagged_interactive_nodes(&mut self, screen: Rect) -> Vec<InspectNode> {
        let _ = draw_desktop(self.desktop, screen);
        let tree = build_desktop_tree(self.desktop, screen);
        let mut nodes = Vec::new();
        collect_untagged_interactive_nodes(&tree, &mut nodes);
        nodes
    }

    pub fn set_property(
        &mut self,
        id: &str,
        name: &str,
        value: ComponentValue,
    ) -> Result<(), ComponentError> {
        let handled = match resolve_dispatch_target(self.desktop, id) {
            Some(DispatchTarget::Menu) => {
                menu_set_property(&mut self.desktop.menu, id, name, value)?
            }
            Some(DispatchTarget::Window) => {
                window_set_property(&mut self.desktop.wm, id, name, value)?
            }
            Some(DispatchTarget::Component) => {
                component_set_property(&mut self.desktop.wm, id, name, value)?
            }
            None => return Err(ComponentError::not_found(id)),
        };
        if handled {
            Ok(())
        } else {
            Err(ComponentError::not_found(id))
        }
    }

    pub fn action(
        &mut self,
        screen: Rect,
        id: &str,
        action: ComponentCommand,
    ) -> Result<EventResult, ComponentError> {
        self.action_target(screen, ComponentTarget::Id(id.to_string()), action)
    }

    pub fn action_target(
        &mut self,
        screen: Rect,
        target: ComponentTarget,
        action: ComponentCommand,
    ) -> Result<EventResult, ComponentError> {
        match target {
            ComponentTarget::Id(id) => Ok(self.invoke_by_id(screen, &id, action)?.result),
            ComponentTarget::Focused => self.action_focused(action),
        }
    }

    pub fn invoke(
        &mut self,
        screen: Rect,
        target: ComponentTarget,
        action: ComponentCommand,
    ) -> Result<InvokeResult, ComponentError> {
        match target {
            ComponentTarget::Id(id) => self.invoke_by_id(screen, &id, action),
            ComponentTarget::Focused => self.invoke_focused(action),
        }
    }

    pub fn wait_for(
        &mut self,
        screen: Rect,
        condition: WaitCondition,
        timeout: Duration,
    ) -> Result<WaitResult, ComponentError> {
        self.wait_for_with_interval(screen, condition, timeout, Duration::from_millis(10))
    }

    pub fn wait_for_with_interval(
        &mut self,
        screen: Rect,
        condition: WaitCondition,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<WaitResult, ComponentError> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .unwrap_or_else(Instant::now);
        let mut tracker = self.change_tracker();
        let mut polls = 0;

        loop {
            draw_desktop(self.desktop, screen)?;
            self.refresh_change_tracker(&mut tracker);
            polls += 1;

            if let Some(value) = self.evaluate_wait_condition(&condition)? {
                return Ok(WaitResult {
                    polls,
                    value: Some(value),
                });
            }

            if Instant::now() >= deadline {
                return Err(ComponentError::timeout(format!(
                    "condition not met after {polls} polls: {condition:?}"
                )));
            }

            sleep_until_next_wait_poll(deadline, poll_interval, &mut tracker);
        }
    }

    pub(crate) fn poll_wait_condition(
        &mut self,
        screen: Rect,
        condition: &WaitCondition,
    ) -> Result<Option<ComponentValue>, ComponentError> {
        draw_desktop(self.desktop, screen)?;
        self.evaluate_wait_condition(condition)
    }

    pub fn wait_for_predicate<F>(
        &mut self,
        screen: Rect,
        timeout: Duration,
        mut predicate: F,
    ) -> Result<WaitResult, ComponentError>
    where
        F: FnMut(&mut Self) -> Result<bool, ComponentError>,
    {
        let deadline = Instant::now()
            .checked_add(timeout)
            .unwrap_or_else(Instant::now);
        let mut tracker = self.change_tracker();
        let mut polls = 0;

        loop {
            draw_desktop(self.desktop, screen)?;
            self.refresh_change_tracker(&mut tracker);
            polls += 1;

            if predicate(self)? {
                return Ok(WaitResult { polls, value: None });
            }

            if Instant::now() >= deadline {
                return Err(ComponentError::timeout(format!(
                    "predicate not met after {polls} polls"
                )));
            }

            sleep_until_next_wait_poll(deadline, poll_interval(), &mut tracker);
        }
    }

    fn evaluate_wait_condition(
        &mut self,
        condition: &WaitCondition,
    ) -> Result<Option<ComponentValue>, ComponentError> {
        match condition {
            WaitCondition::PropertyEquals {
                target,
                property,
                expected,
            } => match self.query(target.clone(), property) {
                Ok(value) if value == *expected => Ok(Some(value)),
                Ok(_) | Err(ComponentError::NotFound(_)) => Ok(None),
                Err(err) => Err(err),
            },
        }
    }

    fn invoke_by_id(
        &mut self,
        screen: Rect,
        id: &str,
        action: ComponentCommand,
    ) -> Result<InvokeResult, ComponentError> {
        let custom_name = match &action {
            ComponentCommand::Custom { name, .. } => Some(name.clone()),
            _ => None,
        };

        let target = resolve_dispatch_target(self.desktop, id);
        let supported = match target {
            Some(DispatchTarget::Menu) => {
                menu_command_supported(&self.desktop.menu, id, &action).unwrap_or(false)
            }
            Some(DispatchTarget::Window) => {
                window_command_supported(&self.desktop.wm, id, &action).unwrap_or(false)
            }
            Some(DispatchTarget::Component) => {
                component_command_supported(&self.desktop.wm, id, &action).unwrap_or(false)
            }
            None => false,
        };
        if supported {
            let result = match target {
                Some(DispatchTarget::Menu) => menu_action(&mut self.desktop.menu, id, &action),
                Some(DispatchTarget::Window) => window_action(&mut self.desktop.wm, id, &action),
                Some(DispatchTarget::Component) => {
                    component_action(&mut self.desktop.wm, id, &action)
                }
                None => None,
            }
            .unwrap_or_else(EventResult::ignored);
            return Ok(InvokeResult::semantic(result));
        }

        // Semantic command not supported by the resolved backend: a Custom command has no
        // coordinate fallback, so a known id reports "action not supported".
        if let Some(name) = custom_name
            && target.is_some()
        {
            return Err(ComponentError::action_not_supported(name));
        }

        match action {
            ComponentCommand::Click | ComponentCommand::Toggle | ComponentCommand::Submit => {
                let snapshot = self.snapshot(screen)?;
                let bounds = snapshot
                    .tree
                    .find_by_id(id)
                    .and_then(|node| node.bounds)
                    .ok_or_else(|| ComponentError::not_found(id))?;
                let (x, y) = center_point(bounds)
                    .ok_or_else(|| ComponentError::action_not_supported("empty bounds"))?;
                let event = Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                });
                let result = self.desktop.handle_event(&event, screen);
                apply_desktop_action(self.desktop, &result.action);
                Ok(InvokeResult::coordinate_fallback(EventResult {
                    outcome: result.outcome,
                    action: crate::composable::ComponentAction::None,
                    capture: crate::composable::Capture::None,
                }))
            }
            ComponentCommand::InputText(text) => {
                let snapshot = self.snapshot(screen)?;
                let bounds = snapshot
                    .tree
                    .find_by_id(id)
                    .and_then(|node| node.bounds)
                    .ok_or_else(|| ComponentError::not_found(id))?;
                if let Some((x, y)) = center_point(bounds) {
                    let click_event = Event::Mouse(MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: x,
                        row: y,
                        modifiers: KeyModifiers::NONE,
                    });
                    let click_result = self.desktop.handle_event(&click_event, screen);
                    apply_desktop_action(self.desktop, &click_result.action);
                }
                let event = Event::Paste(text);
                let result = self.desktop.handle_event(&event, screen);
                apply_desktop_action(self.desktop, &result.action);
                Ok(InvokeResult::coordinate_fallback(EventResult {
                    outcome: result.outcome,
                    action: crate::composable::ComponentAction::None,
                    capture: crate::composable::Capture::None,
                }))
            }
            ComponentCommand::SelectIndex(_) => {
                Err(ComponentError::action_not_supported("SelectIndex"))
            }
            ComponentCommand::Custom { name, .. } => {
                Err(ComponentError::action_not_supported(name))
            }
        }
    }

    fn invoke_focused(&mut self, action: ComponentCommand) -> Result<InvokeResult, ComponentError> {
        let Some(focused) = focused_component_mut(&mut self.desktop.wm) else {
            return Err(ComponentError::not_found("focused"));
        };
        if !focused.supports_command(&action) {
            return match action {
                ComponentCommand::Custom { name, .. } => {
                    Err(ComponentError::action_not_supported(name))
                }
                ComponentCommand::SelectIndex(_) => {
                    Err(ComponentError::action_not_supported("SelectIndex"))
                }
                _ => Ok(InvokeResult::unsupported()),
            };
        }
        let result = focused.apply_command(action);
        Ok(InvokeResult::semantic(result))
    }

    fn action_focused(&mut self, action: ComponentCommand) -> Result<EventResult, ComponentError> {
        let result = self.invoke_focused(action.clone())?.result;
        match action {
            ComponentCommand::Custom { name, .. } => {
                if result.is_consumed() {
                    Ok(result)
                } else {
                    Err(ComponentError::action_not_supported(name))
                }
            }
            ComponentCommand::SelectIndex(_) => {
                if result.is_consumed() {
                    Ok(result)
                } else {
                    Err(ComponentError::action_not_supported("SelectIndex"))
                }
            }
            _ => Ok(result),
        }
    }

    pub fn click(&mut self, screen: Rect, id: &str) -> Result<EventResult, ComponentError> {
        self.action(screen, id, ComponentCommand::Click)
    }

    pub fn input_text(
        &mut self,
        screen: Rect,
        id: &str,
        text: impl Into<String>,
    ) -> Result<EventResult, ComponentError> {
        self.action(screen, id, ComponentCommand::InputText(text.into()))
    }
}

impl Desktop {
    pub fn inspect(&mut self) -> DesktopInspector<'_> {
        DesktopInspector::new(self)
    }
}

#[cfg(test)]
mod tests;
