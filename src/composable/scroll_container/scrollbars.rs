use ratatui::Frame;
use ratatui::layout::Rect;

use super::super::component::{ComponentContext, ScrollbarHost};
use super::super::layout::apply_padding;
use super::super::scroll::{Scrollbars, scrollbar_layout_1d};
use super::{
    ScrollContainer, ScrollContainerScrollbars, ScrollbarLayout, ScrollbarPlacement,
};

impl ScrollContainer {
    pub(super) fn scrollbars_info(&self, scrollbar_host: ScrollbarHost) -> ScrollContainerScrollbars {
        let cfg = self.scroll_config.get();
        let padding = self.padding.get();
        let viewport_size = self.viewport_size.get();
        let content_size = self.content_size.get();
        let scroll = self.scroll.get();
        let thickness = cfg.scrollbar_thickness.max(1);
        let scrollbars = if let Some(scrollbars) = self.scrollbars {
            scrollbars
        } else if let Some(area) = self.last_area {
            let viewport = Rect {
                x: 0,
                y: 0,
                width: area.width,
                height: area.height,
            };
            Scrollbars {
                viewport,
                content: apply_padding(viewport, padding),
                vbar: None,
                hbar: None,
                thickness,
            }
        } else {
            return ScrollContainerScrollbars::default();
        };

        let vbar = (matches!(scrollbar_host, ScrollbarHost::Component))
            .then_some(scrollbars.vbar)
            .flatten()
            .map(|area| {
                let layout = scrollbar_layout_1d(
                    area.height,
                    viewport_size.1,
                    content_size.1,
                    scroll.y,
                    cfg.arrows,
                );
                ScrollbarPlacement {
                    area,
                    layout: ScrollbarLayout::from(layout),
                }
            });
        let hbar = (matches!(scrollbar_host, ScrollbarHost::Component))
            .then_some(scrollbars.hbar)
            .flatten()
            .map(|area| {
                let layout = scrollbar_layout_1d(
                    area.width,
                    viewport_size.0,
                    content_size.0,
                    scroll.x,
                    cfg.arrows,
                );
                ScrollbarPlacement {
                    area,
                    layout: ScrollbarLayout::from(layout),
                }
            });

        ScrollContainerScrollbars {
            viewport: scrollbars.viewport,
            content: scrollbars.content,
            vbar,
            hbar,
            thickness: scrollbars.thickness,
        }
    }

    pub(super) fn draw_scrollbars(&self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let Some(scrollbars) = self.scrollbars else {
            return;
        };

        if !matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
            return;
        }

        let cfg = self.scroll_config.get();
        let viewport_size = self.viewport_size.get();
        let content_size = self.content_size.get();
        let scroll = self.scroll.get();
        super::super::scroll::draw_scrollbars(
            frame,
            area,
            scrollbars,
            viewport_size,
            content_size,
            scroll,
            cfg,
            ctx.theme,
        );
    }
}
