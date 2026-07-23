//! Shared event core for focusable-children containers (`StackCore`, `Grid`).
//!
//! `StackCore` and `Grid` are both "a `Vec<ComponentNode>` with focus traversal, scroll and pointer
//! capture". Their event handling was historically copied verbatim between the two files, which is
//! how they drifted apart — Grid silently lacked pointer capture (a real stuck-button bug) until it
//! was patched back in. This module holds that logic exactly once, as free functions generic over
//! the [`FocusableContainer`] accessor trait, so both containers share one implementation.
//!
//! The trait exposes only the state the event logic touches; layout/drawing and the container-
//! specific fields (`StackCore::axis`, `Grid::columns`, …) stay private to each type.

use crossterm::event::{Event, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::clipped;
use super::component::{Capture, ComponentContext, EventResult, MouseCoordinateSpace, TabMode};
use super::geom::{
    TabDirection, contains, focusable_children_in_tab_order, mouse_coords_local_to_area,
    tab_direction_for_event,
};
use super::node::{ComponentId, ComponentNode};
use super::scroll::{
    ScrollConfig, ScrollOffset, ScrollbarDrag, Scrollbars, clamp_scroll_offset,
    scroll_offset_for_input_event,
};

/// State accessors a focusable-children container must expose for the shared event core.
///
/// Implementors keep the fields; this trait just projects them. Values that live behind a
/// `Binding` are surfaced as resolved reads/writes (e.g. [`scroll`](Self::scroll) /
/// [`set_scroll`](Self::set_scroll)) so the event core never has to know the storage type.
pub(crate) trait FocusableContainer {
    fn children(&self) -> &[ComponentNode];
    fn children_mut(&mut self) -> &mut Vec<ComponentNode>;

    fn focused(&self) -> Option<ComponentId>;
    fn set_focused(&mut self, id: Option<ComponentId>);

    fn captured_child(&self) -> Option<ComponentId>;
    fn set_captured_child(&mut self, id: Option<ComponentId>);

    fn last_area(&self) -> Option<Rect>;

    fn scrollable(&self) -> bool;
    fn scroll(&self) -> ScrollOffset;
    fn set_scroll(&mut self, offset: ScrollOffset);
    fn scroll_config(&self) -> ScrollConfig;
    fn padding(&self) -> super::layout::EdgeInsets;
    fn scrollbars(&self) -> Option<Scrollbars>;
    fn scrollbar_drag_mut(&mut self) -> &mut Option<ScrollbarDrag>;

    fn content_size(&self) -> (u16, u16);
    fn viewport_size(&self) -> (u16, u16);
}

pub(crate) fn first_focusable_child<C: FocusableContainer>(c: &C) -> Option<ComponentId> {
    focusable_children_in_tab_order(c.children()).first().copied()
}

fn move_focus<C: FocusableContainer>(c: &mut C, direction: TabDirection, wrap: bool) -> bool {
    let focusable = focusable_children_in_tab_order(c.children());
    if focusable.is_empty() {
        c.set_focused(None);
        return false;
    }

    let desired = match c
        .focused()
        .and_then(|id| focusable.iter().position(|x| *x == id))
    {
        Some(idx) => match direction {
            TabDirection::Next => {
                if idx + 1 < focusable.len() {
                    Some(focusable[idx + 1])
                } else if wrap {
                    Some(focusable[0])
                } else {
                    None
                }
            }
            TabDirection::Prev => {
                if idx > 0 {
                    Some(focusable[idx - 1])
                } else if wrap {
                    Some(focusable[focusable.len() - 1])
                } else {
                    None
                }
            }
        },
        None => Some(match direction {
            TabDirection::Next => focusable[0],
            TabDirection::Prev => focusable[focusable.len() - 1],
        }),
    };

    let Some(id) = desired else {
        return false;
    };

    c.set_focused(Some(id));
    true
}

fn focus_focused_child_edge<C: FocusableContainer>(c: &mut C, direction: TabDirection) {
    let Some(child_id) = c.focused() else {
        return;
    };
    let Some(child_idx) = c.children().iter().position(|n| n.id == child_id) else {
        return;
    };

    match direction {
        TabDirection::Next => {
            let _ = c.children_mut()[child_idx].view.focus_first();
        }
        TabDirection::Prev => {
            let _ = c.children_mut()[child_idx].view.focus_last();
        }
    }
}

fn handle_tab_navigation<C: FocusableContainer>(
    c: &mut C,
    event: &Event,
    ctx: ComponentContext<'_>,
) -> EventResult {
    let Some(direction) = tab_direction_for_event(event) else {
        return EventResult::ignored();
    };

    if !ctx.is_focused {
        return EventResult::ignored();
    }

    // If we don't have a focused child yet, initialize focus and stop.
    let focusable = focusable_children_in_tab_order(c.children());
    if focusable.is_empty() {
        c.set_focused(None);
        return EventResult::ignored();
    }

    let focused = match c.focused() {
        Some(id) if focusable.contains(&id) => id,
        _ => {
            let id = match direction {
                TabDirection::Next => focusable[0],
                TabDirection::Prev => focusable[focusable.len() - 1],
            };
            c.set_focused(Some(id));
            focus_focused_child_edge(c, direction);
            return EventResult::consumed();
        }
    };

    // Give the currently focused child a chance to advance focus within its subtree.
    if let Some(child_idx) = c.children().iter().position(|n| n.id == focused) {
        let child_focused = ctx.is_focused && c.focused() == Some(focused);
        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: child_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode.for_child(),
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: None,
        };

        let res = c.children_mut()[child_idx]
            .view
            .handle_event_capture(event, child_ctx);
        if res.is_consumed() {
            return res;
        }
    }

    let wrap = matches!(ctx.tab_mode, TabMode::Cycle);
    if move_focus(c, direction, wrap) {
        focus_focused_child_edge(c, direction);
        return EventResult::consumed();
    }

    EventResult::ignored()
}

pub(crate) fn scroll_to_clamped<C: FocusableContainer>(c: &mut C, x: u16, y: u16) -> bool {
    if !c.scrollable() {
        return false;
    }
    let scroll = c.scroll();
    let desired = ScrollOffset { x, y };
    let clamped = clamp_scroll_offset(c.content_size(), c.viewport_size(), desired);
    let changed = clamped != scroll;
    c.set_scroll(clamped);
    changed
}

pub(crate) fn bounds_intersects_viewport(
    bounds: Rect,
    scroll: ScrollOffset,
    viewport: (u16, u16),
) -> bool {
    clipped::bounds_intersects_viewport(bounds, scroll, viewport)
}

fn hit_test_child_scrolled<C: FocusableContainer>(
    c: &C,
    viewport_x: u16,
    viewport_y: u16,
    viewport: (u16, u16),
) -> Option<ComponentId> {
    // Anchored children are treated as overlays and do not scroll.
    for child in c.children().iter().rev().filter(|n| n.layout.anchor.is_some()) {
        if contains(child.bounds(), viewport_x, viewport_y) {
            return Some(child.id);
        }
    }

    let scroll = c.scroll();
    let content_x = viewport_x.saturating_add(scroll.x);
    let content_y = viewport_y.saturating_add(scroll.y);

    for child in c.children().iter().rev().filter(|n| n.layout.anchor.is_none()) {
        if !bounds_intersects_viewport(child.bounds(), scroll, viewport) {
            continue;
        }
        if contains(child.bounds(), content_x, content_y) {
            return Some(child.id);
        }
    }
    None
}

/// Translates a mouse event (in `space`) into the local coordinate space of the child at `idx`,
/// mirroring the hit-test route's math. A pointer outside the child on the low side maps to
/// `u16::MAX` (definitely out of range) so the child can still detect a release outside its own
/// bounds.
fn translate_to_child<C: FocusableContainer>(
    c: &C,
    m: MouseEvent,
    space: MouseCoordinateSpace,
    idx: usize,
) -> (u16, u16) {
    let Some(area) = c.last_area() else {
        return (u16::MAX, u16::MAX);
    };

    // Pointer in area-local space (signed; the pointer may be outside).
    let (local_x, local_y) = match space {
        MouseCoordinateSpace::Absolute => (
            i32::from(m.column) - i32::from(area.x),
            i32::from(m.row) - i32::from(area.y),
        ),
        MouseCoordinateSpace::Local => (i32::from(m.column), i32::from(m.row)),
    };

    let cfg = c.scroll_config();
    let padding = c.padding();
    let thickness = cfg.scrollbar_thickness.max(1);
    let scrollbars = super::scroll::scrollbars_for_event(area, padding, thickness, c.scrollbars());
    let content = scrollbars.content;

    let child = &c.children()[idx];
    let bounds = child.bounds();
    let is_anchored = child.layout.anchor.is_some();
    let scroll = c.scroll();

    let content_x = local_x - i32::from(content.x);
    let content_y = local_y - i32::from(content.y);
    let point_x = if is_anchored {
        content_x
    } else {
        content_x + i32::from(scroll.x)
    };
    let point_y = if is_anchored {
        content_y
    } else {
        content_y + i32::from(scroll.y)
    };
    let child_x = point_x - i32::from(bounds.x);
    let child_y = point_y - i32::from(bounds.y);

    let clamp = |v: i32| {
        if (0..=i32::from(u16::MAX)).contains(&v) {
            v as u16
        } else {
            u16::MAX
        }
    };
    (clamp(child_x), clamp(child_y))
}

pub(crate) fn handle_event_capture<C: FocusableContainer>(
    c: &mut C,
    event: &Event,
    ctx: ComponentContext<'_>,
) -> EventResult {
    let tab = handle_tab_navigation(c, event, ctx);
    if tab.is_consumed() {
        return tab;
    }

    EventResult::ignored()
}

pub(crate) fn handle_event_bubble<C: FocusableContainer>(
    c: &mut C,
    event: &Event,
    coordinate_space: MouseCoordinateSpace,
) -> EventResult {
    if !c.scrollable() {
        return EventResult::ignored();
    }

    if let Event::Mouse(m) = event {
        let Some(area) = c.last_area() else {
            return EventResult::ignored();
        };
        if mouse_coords_local_to_area(area, *m, coordinate_space).is_none() {
            return EventResult::ignored();
        }
    }

    let scroll = c.scroll();
    let Some(new_scroll) = scroll_offset_for_input_event(
        c.scroll_config(),
        c.content_size(),
        c.viewport_size(),
        scroll,
        event,
    ) else {
        return EventResult::ignored();
    };

    if new_scroll == scroll {
        EventResult::ignored()
    } else {
        c.set_scroll(new_scroll);
        EventResult::consumed()
    }
}

pub(crate) fn handle_event<C: FocusableContainer>(
    c: &mut C,
    event: &Event,
    ctx: ComponentContext<'_>,
) -> EventResult {
    let capture = handle_event_capture(c, event, ctx);
    if capture.is_consumed() {
        return capture;
    }

    if let Event::Mouse(m) = event {
        // Pointer capture: route the event straight to the captured child, bypassing
        // scrollbar/hit-test. Coordinates are translated into the child's local space (and
        // forwarded as `Local`) exactly like the hit-test route below, so capture keeps working
        // when the child was drawn through the clipped/offscreen path (where its `last_area` is
        // offscreen-local rather than absolute). A point outside the child still maps to
        // out-of-range local coordinates, so the child can detect a release outside its own bounds.
        if let Some(cap_id) = c.captured_child() {
            if let Some(idx) = c.children().iter().position(|n| n.id == cap_id) {
                let (child_x, child_y) = translate_to_child(c, *m, ctx.mouse_coordinate_space, idx);
                let cap_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused && c.focused() == Some(cap_id),
                    scrollbar_host: ctx.scrollbar_host.for_child(),
                    tab_mode: ctx.tab_mode.for_child(),
                    mouse_coordinate_space: MouseCoordinateSpace::Local,
                    drag: None,
                };
                let child_event = Event::Mouse(MouseEvent {
                    column: child_x,
                    row: child_y,
                    ..*m
                });
                let res = c.children_mut()[idx].view.handle_event(&child_event, cap_ctx);
                if matches!(res.capture, Capture::Release) {
                    c.set_captured_child(None);
                }
                return res;
            }
            c.set_captured_child(None);
        }
    }

    if let Event::Mouse(m) = event {
        let Some(area) = c.last_area() else {
            return handle_event_bubble(c, event, ctx.mouse_coordinate_space);
        };
        let Some((local_x, local_y)) =
            mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
        else {
            return handle_event_bubble(c, event, ctx.mouse_coordinate_space);
        };

        let cfg = c.scroll_config();
        let padding = c.padding();
        let thickness = cfg.scrollbar_thickness.max(1);
        let scrollbars = super::scroll::scrollbars_for_event(area, padding, thickness, c.scrollbars());

        if c.scrollable() {
            let scroll = c.scroll();
            if let Some(new_scroll) = super::scroll::handle_scrollbar_mouse_event(
                cfg,
                scrollbars,
                c.content_size(),
                scroll,
                c.scrollbar_drag_mut(),
                local_x,
                local_y,
                m.kind,
            ) {
                let clamped = clamp_scroll_offset(c.content_size(), c.viewport_size(), new_scroll);
                c.set_scroll(clamped);
                return EventResult::consumed();
            }
        }

        let content = scrollbars.content;
        if !contains(content, local_x, local_y) {
            return handle_event_bubble(c, event, ctx.mouse_coordinate_space);
        }

        let content_x = local_x.saturating_sub(content.x);
        let content_y = local_y.saturating_sub(content.y);
        let content_size = (content.width, content.height);

        let Some(child_id) = hit_test_child_scrolled(c, content_x, content_y, content_size) else {
            return handle_event_bubble(c, event, ctx.mouse_coordinate_space);
        };
        let Some(child_idx) = c.children().iter().position(|n| n.id == child_id) else {
            return handle_event_bubble(c, event, ctx.mouse_coordinate_space);
        };

        let child_bounds = c.children()[child_idx].bounds();
        let is_anchored = c.children()[child_idx].layout.anchor.is_some();
        let scroll = c.scroll();
        let point_x = if is_anchored {
            content_x
        } else {
            content_x.saturating_add(scroll.x)
        };
        let point_y = if is_anchored {
            content_y
        } else {
            content_y.saturating_add(scroll.y)
        };
        let child_x = point_x.saturating_sub(child_bounds.x);
        let child_y = point_y.saturating_sub(child_bounds.y);

        let focus_changed = matches!(m.kind, MouseEventKind::Down(_))
            && c.children()[child_idx].view.is_focusable()
            && c.focused() != Some(child_id);
        if focus_changed {
            c.set_focused(Some(child_id));
        }

        let child_event = Event::Mouse(MouseEvent {
            column: child_x,
            row: child_y,
            ..*m
        });

        let child_focused = ctx.is_focused && c.focused() == Some(child_id);
        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: child_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode.for_child(),
            mouse_coordinate_space: ctx.mouse_coordinate_space.for_child(),
            drag: None,
        };

        let res = c.children_mut()[child_idx]
            .view
            .handle_event(&child_event, child_ctx);
        // A child that requested capture on this (down) event becomes the capture target; the
        // request bubbles further up via the returned result so the window manager can capture at
        // its level too.
        match res.capture {
            Capture::Request => c.set_captured_child(Some(child_id)),
            Capture::Release => c.set_captured_child(None),
            Capture::None => {}
        }
        if res.is_consumed() {
            return res;
        }

        if focus_changed {
            return EventResult::consumed();
        }

        return handle_event_bubble(c, event, ctx.mouse_coordinate_space);
    }

    // Keyboard/paste/etc: send to focused child first.
    if let Some(child_id) = c.focused().or_else(|| first_focusable_child(c))
        && let Some(child_idx) = c.children().iter().position(|n| n.id == child_id)
    {
        c.set_focused(Some(child_id));
        let child_focused = ctx.is_focused && c.focused() == Some(child_id);
        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: child_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode.for_child(),
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: None,
        };
        let res = c.children_mut()[child_idx]
            .view
            .handle_event(event, child_ctx);
        if res.is_consumed() {
            return res;
        }
    }

    handle_event_bubble(c, event, ctx.mouse_coordinate_space)
}
