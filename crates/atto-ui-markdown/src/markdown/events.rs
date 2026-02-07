use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};

use atto_ui::composable::{EventResult, ScrollContentContext, ScrollOffset};

use super::embedded_scrollbar::{
    EmbeddedScrollView, apply_embedded_scrollbar_drag, handle_embedded_scrollbar_mouse_down,
    prefix_width_for_block,
};
use super::layout::{Layout, LayoutBlock, LayoutBlockKind};
use super::{EmbeddedScrollbarTarget, MarkdownShared};

pub(super) fn handle_content_event(
    shared: &mut MarkdownShared,
    event: &Event,
    ctx: ScrollContentContext<'_>,
) -> EventResult {
    let layout = shared.cache.layout.clone();
    let Some(layout) = layout else {
        return EventResult::ignored();
    };
    let scroll = ctx.info.scroll_offset;
    let viewport = ctx.info.viewport_size;
    if viewport.0 == 0 || viewport.1 == 0 {
        return EventResult::ignored();
    }

    let Event::Mouse(m) = event else {
        return EventResult::ignored();
    };

    if shared.embedded_scrollbar_drag.is_some()
        && let Some(res) = shared.handle_embedded_scrollbar_drag(*m, scroll, viewport, &layout)
    {
        return res;
    }

    let content_x = scroll.x.saturating_add(m.column);
    let content_y = scroll.y.saturating_add(m.row);

    if let Some(block_idx) = layout.block_at_row(content_y) {
        let block = &layout.blocks[block_idx];
        let local_y = content_y.saturating_sub(block.y);
        let local_x = content_x;
        if let Some(res) =
            shared.handle_block_event(block, local_x, local_y, *m, viewport, layout.wrap_width)
            && res.is_consumed()
        {
            return res;
        }
    }

    if let MouseEventKind::Down(MouseButton::Left) = m.kind
        && let Some(hit) = layout.link_at(content_x, content_y)
    {
        shared.link_callback.fire(&hit.url);
        return EventResult::consumed();
    }

    EventResult::ignored()
}

impl MarkdownShared {
    fn handle_embedded_scrollbar_drag(
        &mut self,
        m: MouseEvent,
        scroll: ScrollOffset,
        viewport: (u16, u16),
        layout: &Layout,
    ) -> Option<EventResult> {
        let drag = self.embedded_scrollbar_drag?;
        match m.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                let content_x = scroll.x.saturating_add(m.column);
                let content_y = scroll.y.saturating_add(m.row);

                let Some(block) =
                    layout
                        .blocks
                        .iter()
                        .find(|block| match (&block.kind, drag.target) {
                            (
                                LayoutBlockKind::Code { index, .. },
                                EmbeddedScrollbarTarget::Code(id),
                            ) => *index == id,
                            (
                                LayoutBlockKind::Table { index, .. },
                                EmbeddedScrollbarTarget::Table(id),
                            ) => *index == id,
                            _ => false,
                        })
                else {
                    self.embedded_scrollbar_drag = None;
                    return Some(EventResult::consumed());
                };

                let local_x = content_x;
                let local_y = content_y.saturating_sub(block.y);

                let prefix_width = prefix_width_for_block(block);

                match &block.kind {
                    LayoutBlockKind::Code { index, .. } => {
                        let code = self.cache.code_blocks.get_mut(*index)?;
                        let (content_w, content_h) = code.content_size();
                        let (target_scroll, embedded) = solve_embedded_scroll_and_layout(
                            code.scroll,
                            (content_w, content_h),
                            block,
                            viewport,
                            layout.wrap_width,
                        );
                        let new_scroll = apply_embedded_scrollbar_drag(
                            target_scroll,
                            (content_w, content_h),
                            embedded,
                            prefix_width,
                            local_x,
                            local_y,
                            drag.drag,
                        );
                        code.scroll = new_scroll;
                        return Some(EventResult::consumed());
                    }
                    LayoutBlockKind::Table { index, .. } => {
                        let table = self.cache.tables.get_mut(*index)?;
                        let (content_w, content_h) = table.content_size();
                        let (target_scroll, embedded) = solve_embedded_scroll_and_layout(
                            table.scroll,
                            (content_w, content_h),
                            block,
                            viewport,
                            layout.wrap_width,
                        );
                        let new_scroll = apply_embedded_scrollbar_drag(
                            target_scroll,
                            (content_w, content_h),
                            embedded,
                            prefix_width,
                            local_x,
                            local_y,
                            drag.drag,
                        );
                        table.scroll = new_scroll;
                        return Some(EventResult::consumed());
                    }
                    _ => {}
                }

                Some(EventResult::consumed())
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.embedded_scrollbar_drag = None;
                Some(EventResult::consumed())
            }
            _ => None,
        }
    }

    fn handle_block_event(
        &mut self,
        block: &LayoutBlock,
        local_x: u16,
        local_y: u16,
        m: MouseEvent,
        viewport: (u16, u16),
        wrap_width: u16,
    ) -> Option<EventResult> {
        let is_wheel = matches!(
            m.kind,
            MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight
        );

        match &block.kind {
            LayoutBlockKind::Code { index, prefix, .. } => {
                let prefix_width = prefix.first_width.max(prefix.rest_width);
                let total_width = wrap_width.min(viewport.0);
                if total_width == 0 || local_x >= total_width {
                    return None;
                }
                let outer_w = total_width.saturating_sub(prefix_width);
                if outer_w == 0 {
                    return None;
                }

                let code = self.cache.code_blocks.get_mut(*index)?;
                let (content_w, content_h) = code.content_size();
                let embedded =
                    EmbeddedScrollView::solve_auto((content_w, content_h), (outer_w, block.height));

                // Click/drag on embedded scrollbars.
                if let MouseEventKind::Down(MouseButton::Left) = m.kind
                    && let Some(res) = handle_embedded_scrollbar_mouse_down(
                        &mut self.embedded_scrollbar_drag,
                        EmbeddedScrollbarTarget::Code(*index),
                        code.scroll,
                        (content_w, content_h),
                        embedded,
                        local_x,
                        local_y,
                        prefix_width,
                    )
                {
                    code.scroll = res;
                    return Some(EventResult::consumed());
                }

                if is_wheel {
                    let embedded = EmbeddedScrollView::solve_auto(
                        (content_w, content_h),
                        (outer_w, block.height),
                    );
                    let consumed = code.handle_scroll(
                        m,
                        embedded.viewport_w,
                        embedded.viewport_h,
                        super::DEFAULT_SCROLL_STEP,
                    );
                    if consumed {
                        return Some(EventResult::consumed());
                    }
                    return None;
                }
                None
            }
            LayoutBlockKind::Table { index, prefix, .. } => {
                let prefix_width = prefix.first_width.max(prefix.rest_width);
                let total_width = wrap_width.min(viewport.0);
                if total_width == 0 || local_x >= total_width {
                    return None;
                }
                let outer_w = total_width.saturating_sub(prefix_width);
                if outer_w == 0 {
                    return None;
                }

                let table = self.cache.tables.get_mut(*index)?;
                let (content_w, content_h) = table.content_size();
                let embedded =
                    EmbeddedScrollView::solve_auto((content_w, content_h), (outer_w, block.height));

                // Click/drag on embedded scrollbars.
                if let MouseEventKind::Down(MouseButton::Left) = m.kind
                    && let Some(res) = handle_embedded_scrollbar_mouse_down(
                        &mut self.embedded_scrollbar_drag,
                        EmbeddedScrollbarTarget::Table(*index),
                        table.scroll,
                        (content_w, content_h),
                        embedded,
                        local_x,
                        local_y,
                        prefix_width,
                    )
                {
                    table.scroll = res;
                    return Some(EventResult::consumed());
                }

                if is_wheel {
                    let embedded = EmbeddedScrollView::solve_auto(
                        (content_w, content_h),
                        (outer_w, block.height),
                    );
                    let consumed = table.handle_scroll(
                        m,
                        embedded.viewport_w,
                        embedded.viewport_h,
                        super::DEFAULT_SCROLL_STEP,
                    );
                    if consumed {
                        return Some(EventResult::consumed());
                    }
                    return None;
                }

                let content_x = local_x.saturating_sub(prefix_width);

                let max_x = content_w.saturating_sub(embedded.viewport_w);
                let max_y = content_h.saturating_sub(embedded.viewport_h);
                table.scroll.x = table.scroll.x.min(max_x);
                table.scroll.y = table.scroll.y.min(max_y);

                if local_x >= prefix_width
                    && content_x < embedded.viewport_w
                    && local_y < embedded.viewport_h
                    && let MouseEventKind::Down(MouseButton::Left) = m.kind
                    && let Some(url) = table.link_at(content_x, local_y)
                {
                    self.link_callback.fire(&url);
                    return Some(EventResult::consumed());
                }
                None
            }
            _ => None,
        }
    }
}

fn solve_embedded_scroll_and_layout(
    scroll: ScrollOffset,
    content: (u16, u16),
    block: &LayoutBlock,
    viewport: (u16, u16),
    wrap_width: u16,
) -> (ScrollOffset, EmbeddedScrollView) {
    let prefix_width = prefix_width_for_block(block);

    let total_width = wrap_width.min(viewport.0);
    let outer_w = total_width.saturating_sub(prefix_width);
    let outer_h = block.height;

    let embedded = EmbeddedScrollView::solve_auto(content, (outer_w, outer_h));
    let max_x = content.0.saturating_sub(embedded.viewport_w);
    let max_y = content.1.saturating_sub(embedded.viewport_h);
    (
        ScrollOffset {
            x: scroll.x.min(max_x),
            y: scroll.y.min(max_y),
        },
        embedded,
    )
}
