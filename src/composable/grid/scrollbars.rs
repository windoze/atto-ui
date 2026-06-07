use ratatui::Frame;
use ratatui::layout::Rect;

use super::super::clipped::{draw_component_region, scrolled_region};
use super::super::component::{ComponentContext, ScrollbarHost};
use super::super::scroll::{clamp_scroll_offset, draw_scrollbars, resolve_scroll_view};
use super::Grid;

impl Grid {
    pub(super) fn draw_impl(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ComponentContext<'_>,
    ) {
        self.last_area = Some(area);

        let cfg = self.scroll_config.get();
        let padding = self.padding.get();
        let scrollable = self.scrollable.get();
        let host_scrollbars = matches!(ctx.scrollbar_host, ScrollbarHost::Component);

        let resolved = resolve_scroll_view(
            area,
            padding,
            cfg,
            scrollable,
            host_scrollbars,
            |viewport_size| self.layout_children(viewport_size),
        );

        self.viewport_size = resolved.viewport_size;
        self.content_size = resolved.content_size;
        let inner = resolved.inner;

        self.scrollbars = resolved.scrollbars;
        if self.scrollbars.is_none() || (!resolved.show_v && !resolved.show_h) {
            self.scrollbar_drag = None;
        }

        let scroll = self.scroll.get();
        self.scroll.set(clamp_scroll_offset(
            self.content_size,
            self.viewport_size,
            scroll,
        ));

        let scrollable = self.scrollable.get();
        let scroll = self.scroll.get();
        let viewport_size = self.viewport_size;

        for child in self
            .children
            .iter_mut()
            .filter(|c| c.layout.anchor.is_none())
        {
            let r = child.bounds();
            if r.width == 0 || r.height == 0 {
                continue;
            }
            let child_focused = ctx.is_focused && self.focused == Some(child.id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
                mouse_coordinate_space: ctx.mouse_coordinate_space,
                drag: None,
            };
            if scrollable {
                let Some(region) = scrolled_region(r, scroll, viewport_size, inner) else {
                    continue;
                };
                let component_area = Rect::new(0, 0, r.width, r.height);
                if region.source == component_area {
                    child.view.draw(frame, region.dest, child_ctx);
                } else {
                    draw_component_region(
                        frame,
                        child.view.as_mut(),
                        component_area,
                        region.source,
                        region.dest,
                        child_ctx,
                    );
                }
            } else {
                let abs = Rect {
                    x: inner.x.saturating_add(r.x),
                    y: inner.y.saturating_add(r.y),
                    width: r.width,
                    height: r.height,
                };
                child.view.draw(frame, abs, child_ctx);
            }
        }

        for child in self
            .children
            .iter_mut()
            .filter(|c| c.layout.anchor.is_some())
        {
            let r = child.bounds();
            if r.width == 0 || r.height == 0 {
                continue;
            }
            let abs = Rect {
                x: inner.x.saturating_add(r.x),
                y: inner.y.saturating_add(r.y),
                width: r.width,
                height: r.height,
            };

            let child_focused = ctx.is_focused && self.focused == Some(child.id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
                mouse_coordinate_space: ctx.mouse_coordinate_space,
                drag: None,
            };
            child.view.draw(frame, abs, child_ctx);
        }

        let Some(scrollbars) = self.scrollbars else {
            return;
        };

        draw_scrollbars(
            frame,
            area,
            scrollbars,
            viewport_size,
            self.content_size,
            scroll,
            cfg,
            ctx.theme,
        );
    }
}
