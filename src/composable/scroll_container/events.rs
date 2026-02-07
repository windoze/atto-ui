use super::super::component::{ComponentContext, EventResult, ScrollbarHost};
use super::super::geom::{contains, mouse_coords_local_to_area};
use super::super::layout::add_signed;
use super::super::scroll::{ScrollOffset, clamp_scroll_offset, max_scroll_offset};
use super::{ScrollContainer, ScrollContainerHost, ScrollContentContext};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent};

impl ScrollContainer {
    fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        let scroll = self.scroll.get();
        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        let desired = ScrollOffset {
            x: add_signed(scroll.x, dx),
            y: add_signed(scroll.y, dy),
        };
        let clamped = clamp_scroll_offset(content_size, viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

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

    fn handle_event_bubble_impl(&mut self, event: &Event) -> EventResult {
        let cfg = self.scroll_config.get();
        let viewport_size = self.viewport_size.get();
        let content_size = self.content_size.get();
        match event {
            Event::Key(KeyEvent { code, kind, .. }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }

                let viewport_h = viewport_size.1;
                let max = max_scroll_offset(content_size, viewport_size);

                let changed = match code {
                    KeyCode::Up => self.scroll_by(0, -1),
                    KeyCode::Down => self.scroll_by(0, 1),
                    KeyCode::Left => self.scroll_by(-1, 0),
                    KeyCode::Right => self.scroll_by(1, 0),
                    KeyCode::PageUp => self.scroll_by(0, -(viewport_h as i16)),
                    KeyCode::PageDown => self.scroll_by(0, viewport_h as i16),
                    KeyCode::Home => self.scroll_to_clamped(0, 0),
                    KeyCode::End => self.scroll_to_clamped(max.x, max.y),
                    _ => false,
                };

                if changed {
                    EventResult::consumed()
                } else {
                    EventResult::ignored()
                }
            }
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                if mouse_coords_local_to_area(area, *m).is_none() {
                    return EventResult::ignored();
                }

                let step = cfg.wheel_step as i16;
                let changed = match m.kind {
                    crossterm::event::MouseEventKind::ScrollUp => self.scroll_by(0, -step),
                    crossterm::event::MouseEventKind::ScrollDown => self.scroll_by(0, step),
                    crossterm::event::MouseEventKind::ScrollLeft => self.scroll_by(-step, 0),
                    crossterm::event::MouseEventKind::ScrollRight => self.scroll_by(step, 0),
                    _ => false,
                };

                if changed {
                    EventResult::consumed()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
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
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
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
                return self.handle_event_bubble_impl(event);
            };
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return self.handle_event_bubble_impl(event);
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

        self.handle_event_bubble_impl(event)
    }
}
