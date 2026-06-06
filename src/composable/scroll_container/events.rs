use super::super::component::{ComponentContext, EventResult, MouseCoordinateSpace, ScrollbarHost};
use super::super::geom::{contains, mouse_coords_local_to_area};
use super::super::scroll::{ScrollOffset, clamp_scroll_offset, scroll_offset_for_input_event};
use super::{ScrollContainer, ScrollContainerHost, ScrollContentContext};
use crossterm::event::{Event, MouseEvent};

impl ScrollContainer {
    pub(super) fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        let desired = ScrollOffset { x, y };
        let scroll = self.scroll.get();
        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        let clamped = clamp_scroll_offset(content_size, viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

    fn handle_event_bubble_impl(
        &mut self,
        event: &Event,
        coordinate_space: MouseCoordinateSpace,
    ) -> EventResult {
        if let Event::Mouse(m) = event {
            let Some(area) = self.last_area else {
                return EventResult::ignored();
            };
            if mouse_coords_local_to_area(area, *m, coordinate_space).is_none() {
                return EventResult::ignored();
            }
        }

        let scroll = self.scroll.get();
        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        let Some(new_scroll) = scroll_offset_for_input_event(
            self.scroll_config.get(),
            content_size,
            viewport_size,
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
        let cfg = self.scroll_config.get();
        let padding = self.padding.get();
        let viewport_size = self.viewport_size.get();
        let content_size = self.content_size.get();
        let thickness = cfg.scrollbar_thickness.max(1);

        let info = self.info(ctx.scrollbar_host);
        let content_ctx = ScrollContentContext {
            component: ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: ctx.is_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode,
                mouse_coordinate_space: ctx.mouse_coordinate_space.for_child(),
            },
            info,
        };

        // Scrollbar hit-testing is only relevant when scrollbars are hosted by the view itself.
        if matches!(ctx.scrollbar_host, ScrollbarHost::Component)
            && let Event::Mouse(m) = event
        {
            let Some(area) = self.last_area else {
                return EventResult::ignored();
            };
            let Some((local_x, local_y)) =
                mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
            else {
                return EventResult::ignored();
            };

            let scrollbars = super::super::scroll::scrollbars_for_event(
                area,
                padding,
                thickness,
                self.scrollbars,
            );
            let scroll = self.scroll.get();
            if let Some(new_scroll) = super::super::scroll::handle_scrollbar_mouse_event(
                cfg,
                scrollbars,
                content_size,
                scroll,
                &mut self.scrollbar_drag,
                local_x,
                local_y,
                m.kind,
            ) {
                self.scroll
                    .set(clamp_scroll_offset(content_size, viewport_size, new_scroll));
                return EventResult::consumed();
            }
        }

        // Content receives events first.
        if let Event::Mouse(m) = event {
            let Some(area) = self.last_area else {
                return self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space);
            };
            let Some((local_x, local_y)) =
                mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
            else {
                return self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space);
            };

            let scrollbars = super::super::scroll::scrollbars_for_event(
                area,
                padding,
                thickness,
                self.scrollbars,
            );

            let content = scrollbars.content;
            if contains(content, local_x, local_y) {
                let child_event = Event::Mouse(MouseEvent {
                    column: local_x.saturating_sub(content.x),
                    row: local_y.saturating_sub(content.y),
                    ..*m
                });

                let mut host = ScrollContainerHost {
                    scroll: self.scroll.clone(),
                    content_size: self.content_size.clone(),
                    viewport_size: self.viewport_size.clone(),
                };
                self.content.on_scrollbars(content_ctx, &mut host);
                let res = self
                    .content
                    .handle_event(&child_event, content_ctx, &mut host);
                if res.is_consumed() {
                    return res;
                }
            }
        } else {
            let mut host = ScrollContainerHost {
                scroll: self.scroll.clone(),
                content_size: self.content_size.clone(),
                viewport_size: self.viewport_size.clone(),
            };
            self.content.on_scrollbars(content_ctx, &mut host);
            let res = self.content.handle_event(event, content_ctx, &mut host);
            if res.is_consumed() {
                return res;
            }
        }

        self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space)
    }
}
