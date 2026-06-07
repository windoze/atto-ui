use crossterm::event::{Event, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::super::clipped;
use super::super::component::{ComponentContext, EventResult, MouseCoordinateSpace, TabMode};
use super::super::geom::{
    TabDirection, contains, focusable_children_in_tab_order, mouse_coords_local_to_area,
    tab_direction_for_event,
};
use super::super::node::ComponentId;
use super::super::scroll::{ScrollOffset, clamp_scroll_offset, scroll_offset_for_input_event};
use super::Grid;

impl Grid {
    fn first_focusable_child(&self) -> Option<ComponentId> {
        focusable_children_in_tab_order(&self.children)
            .first()
            .copied()
    }

    fn move_focus(&mut self, direction: TabDirection, wrap: bool) -> bool {
        let focusable = focusable_children_in_tab_order(&self.children);
        if focusable.is_empty() {
            self.focused = None;
            return false;
        }

        let desired = match self
            .focused
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

        self.focused = Some(id);
        true
    }

    fn focus_focused_child_edge(&mut self, direction: TabDirection) {
        let Some(child_id) = self.focused else {
            return;
        };
        let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) else {
            return;
        };

        match direction {
            TabDirection::Next => {
                let _ = self.children[child_idx].view.focus_first();
            }
            TabDirection::Prev => {
                let _ = self.children[child_idx].view.focus_last();
            }
        }
    }

    fn handle_tab_navigation(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(direction) = tab_direction_for_event(event) else {
            return EventResult::ignored();
        };

        if !ctx.is_focused {
            return EventResult::ignored();
        }

        // If we don't have a focused child yet, initialize focus and stop.
        let focusable = focusable_children_in_tab_order(&self.children);
        if focusable.is_empty() {
            self.focused = None;
            return EventResult::ignored();
        }

        let focused = match self.focused {
            Some(id) if focusable.contains(&id) => id,
            _ => {
                let id = match direction {
                    TabDirection::Next => focusable[0],
                    TabDirection::Prev => focusable[focusable.len() - 1],
                };
                self.focused = Some(id);
                self.focus_focused_child_edge(direction);
                return EventResult::consumed();
            }
        };

        // Give the currently focused child a chance to advance focus within its subtree.
        if let Some(child_idx) = self.children.iter().position(|c| c.id == focused) {
            let child_focused = ctx.is_focused && self.focused == Some(focused);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
                mouse_coordinate_space: ctx.mouse_coordinate_space,
                drag: None,
            };

            let res = self.children[child_idx]
                .view
                .handle_event_capture(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
        }

        let wrap = matches!(ctx.tab_mode, TabMode::Cycle);
        if self.move_focus(direction, wrap) {
            self.focus_focused_child_edge(direction);
            return EventResult::consumed();
        }

        EventResult::ignored()
    }

    pub(super) fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        if !self.scrollable.get() {
            return false;
        }
        let scroll = self.scroll.get();
        let desired = ScrollOffset { x, y };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

    pub(super) fn bounds_intersects_viewport(
        bounds: Rect,
        scroll: ScrollOffset,
        viewport: (u16, u16),
    ) -> bool {
        clipped::bounds_intersects_viewport(bounds, scroll, viewport)
    }

    fn hit_test_child_scrolled(
        &self,
        viewport_x: u16,
        viewport_y: u16,
        viewport: (u16, u16),
    ) -> Option<ComponentId> {
        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_some())
        {
            if contains(child.bounds(), viewport_x, viewport_y) {
                return Some(child.id);
            }
        }

        let scroll = self.scroll.get();
        let content_x = viewport_x.saturating_add(scroll.x);
        let content_y = viewport_y.saturating_add(scroll.y);

        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_none())
        {
            if !Self::bounds_intersects_viewport(child.bounds(), scroll, viewport) {
                continue;
            }
            if contains(child.bounds(), content_x, content_y) {
                return Some(child.id);
            }
        }
        None
    }

    pub(super) fn handle_event_capture_impl(
        &mut self,
        event: &Event,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        let tab = self.handle_tab_navigation(event, ctx);
        if tab.is_consumed() {
            return tab;
        }

        EventResult::ignored()
    }

    pub(super) fn handle_event_bubble_impl(
        &mut self,
        event: &Event,
        coordinate_space: MouseCoordinateSpace,
    ) -> EventResult {
        if !self.scrollable.get() {
            return EventResult::ignored();
        }

        if let Event::Mouse(m) = event {
            let Some(area) = self.last_area else {
                return EventResult::ignored();
            };
            if mouse_coords_local_to_area(area, *m, coordinate_space).is_none() {
                return EventResult::ignored();
            }
        }

        let scroll = self.scroll.get();
        let Some(new_scroll) = scroll_offset_for_input_event(
            self.scroll_config.get(),
            self.content_size,
            self.viewport_size,
            scroll,
            event,
        ) else {
            return EventResult::ignored();
        };

        if new_scroll == scroll {
            EventResult::ignored()
        } else {
            self.scroll.set(new_scroll);
            EventResult::consumed()
        }
    }

    pub(super) fn handle_event_impl(
        &mut self,
        event: &Event,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        let capture = self.handle_event_capture_impl(event, ctx);
        if capture.is_consumed() {
            return capture;
        }

        if let Event::Mouse(m) = event {
            let Some(area) = self.last_area else {
                return self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space);
            };
            let Some((local_x, local_y)) =
                mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
            else {
                return self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space);
            };

            let cfg = self.scroll_config.get();
            let padding = self.padding.get();
            let thickness = cfg.scrollbar_thickness.max(1);
            let scrollbars = super::super::scroll::scrollbars_for_event(
                area,
                padding,
                thickness,
                self.scrollbars,
            );

            if self.scrollable.get() {
                let scroll = self.scroll.get();
                if let Some(new_scroll) = super::super::scroll::handle_scrollbar_mouse_event(
                    cfg,
                    scrollbars,
                    self.content_size,
                    scroll,
                    &mut self.scrollbar_drag,
                    local_x,
                    local_y,
                    m.kind,
                ) {
                    self.scroll.set(clamp_scroll_offset(
                        self.content_size,
                        self.viewport_size,
                        new_scroll,
                    ));
                    return EventResult::consumed();
                }
            }

            let content = scrollbars.content;
            if !contains(content, local_x, local_y) {
                return self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space);
            }

            let content_x = local_x.saturating_sub(content.x);
            let content_y = local_y.saturating_sub(content.y);
            let content_size = (content.width, content.height);

            let Some(child_id) = self.hit_test_child_scrolled(content_x, content_y, content_size)
            else {
                return self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space);
            };
            let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) else {
                return self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space);
            };

            let child_bounds = self.children[child_idx].bounds();
            let is_anchored = self.children[child_idx].layout.anchor.is_some();
            let scroll = self.scroll.get();
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
                && self.children[child_idx].view.is_focusable()
                && self.focused != Some(child_id);
            if focus_changed {
                self.focused = Some(child_id);
            }

            let child_event = Event::Mouse(MouseEvent {
                column: child_x,
                row: child_y,
                ..*m
            });

            let child_focused = ctx.is_focused && self.focused == Some(child_id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
                mouse_coordinate_space: ctx.mouse_coordinate_space.for_child(),
                drag: None,
            };

            let res = self.children[child_idx]
                .view
                .handle_event(&child_event, child_ctx);
            if res.is_consumed() {
                return res;
            }

            if focus_changed {
                return EventResult::consumed();
            }

            return self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space);
        }

        if let Some(child_id) = self.focused.or_else(|| self.first_focusable_child())
            && let Some(child_idx) = self.children.iter().position(|c| c.id == child_id)
        {
            self.focused = Some(child_id);
            let child_focused = ctx.is_focused && self.focused == Some(child_id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
                mouse_coordinate_space: ctx.mouse_coordinate_space,
                drag: None,
            };
            let res = self.children[child_idx].view.handle_event(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
        }

        self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space)
    }
}
