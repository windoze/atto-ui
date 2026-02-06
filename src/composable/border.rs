use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders};

use super::component::{Component, ComponentContext, EventResult, ScrollbarHost};
use super::node::ComponentId;
use super::scroll::ScrollConfig;
use crate::reactive::Binding;

use super::scroll::{
    ScrollbarDrag, ScrollbarHit, scroll_offset_from_thumb_start, scrollbar_hit_test,
    scrollbar_layout_1d, should_show_scrollbar,
};

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
    if contains(area, m.column, m.row) {
        return Some((
            m.column.saturating_sub(area.x),
            m.row.saturating_sub(area.y),
        ));
    }

    // Nested containers may forward mouse coordinates already relative to this view.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}

fn inset(area: Rect, n: u16) -> Rect {
    if area.width <= 2 * n || area.height <= 2 * n {
        return Rect::default();
    }
    Rect {
        x: area.x.saturating_add(n),
        y: area.y.saturating_add(n),
        width: area.width.saturating_sub(2 * n),
        height: area.height.saturating_sub(2 * n),
    }
}

/// Adds an optional border around an arbitrary [`Component`].
///
/// This is the generic mechanism behind “all components can have optional borders”.
pub struct Border {
    inner: Box<dyn Component>,
    border: Binding<bool>,
    last_area: Option<Rect>,
    scrollbar_drag: Option<ScrollbarDrag>,
}

impl Border {
    pub fn new(inner: Box<dyn Component>) -> Self {
        Self {
            inner,
            border: true.into(),
            last_area: None,
            scrollbar_drag: None,
        }
    }

    pub fn with_border(mut self, border: impl Into<Binding<bool>>) -> Self {
        self.border = border.into();
        self
    }
}

impl Component for Border {
    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.inner.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.inner.focus_last()
    }

    fn min_width(&self) -> u16 {
        let inner = self.inner.min_width();
        if self.border.get() {
            inner.saturating_add(2).max(3)
        } else {
            inner
        }
    }

    fn min_height(&self) -> u16 {
        let inner = self.inner.min_height();
        if self.border.get() {
            inner.saturating_add(2).max(3)
        } else {
            inner
        }
    }

    fn desired_width(&self) -> Option<u16> {
        let w = self.inner.desired_width()?;
        Some(w.saturating_add(if self.border.get() { 2 } else { 0 }))
    }

    fn desired_height(&self) -> Option<u16> {
        let h = self.inner.desired_height()?;
        Some(h.saturating_add(if self.border.get() { 2 } else { 0 }))
    }

    fn is_scrollable(&self) -> bool {
        self.inner.is_scrollable()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.inner.scroll_config()
    }

    fn content_size(&self) -> (u16, u16) {
        self.inner.content_size()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.inner.viewport_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.inner.scroll_offset()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.inner.set_scroll_offset(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        self.inner.scroll_to_child(child_id);
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let border = self.border.get();
        let host_scrollbars = border
            && matches!(ctx.scrollbar_host, ScrollbarHost::Component)
            && self.inner.is_scrollable();
        let inner_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: if host_scrollbars {
                ScrollbarHost::Window
            } else {
                ctx.scrollbar_host
            },
            tab_mode: ctx.tab_mode,
        };

        if let Event::Mouse(m) = event {
            let Some(area) = self.last_area else {
                return EventResult::ignored();
            };
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return EventResult::ignored();
            };

            let local_area = Rect {
                x: 0,
                y: 0,
                width: area.width,
                height: area.height,
            };
            let inner_local = if border {
                inset(local_area, 1)
            } else {
                local_area
            };
            if inner_local.width == 0 || inner_local.height == 0 {
                return EventResult::ignored();
            }

            if host_scrollbars {
                let cfg = self.inner.scroll_config();
                let (content_w, content_h) = self.inner.content_size();
                let (viewport_w, viewport_h) = self.inner.viewport_size();
                let (scroll_x, scroll_y) = self.inner.scroll_offset();

                let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
                let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);

                let vbar = show_v.then_some(Rect {
                    x: local_area.width.saturating_sub(1),
                    y: inner_local.y,
                    width: 1,
                    height: inner_local.height,
                });
                let hbar = show_h.then_some(Rect {
                    x: inner_local.x,
                    y: local_area.height.saturating_sub(1),
                    width: inner_local.width,
                    height: 1,
                });

                if let Some(drag) = self.scrollbar_drag {
                    match m.kind {
                        MouseEventKind::Drag(MouseButton::Left) => match drag {
                            ScrollbarDrag::Vertical { grab_offset } => {
                                let Some(vbar) = vbar else {
                                    self.scrollbar_drag = None;
                                    return EventResult::consumed();
                                };
                                if vbar.height == 0 {
                                    return EventResult::consumed();
                                }

                                let layout = scrollbar_layout_1d(
                                    vbar.height,
                                    viewport_h,
                                    content_h,
                                    scroll_y,
                                    cfg.arrows,
                                );
                                if layout.track_len == 0 {
                                    return EventResult::consumed();
                                }

                                let pos = local_y
                                    .saturating_sub(vbar.y)
                                    .min(vbar.height.saturating_sub(1));
                                let pos_in_track = pos
                                    .saturating_sub(layout.track_start)
                                    .min(layout.track_len.saturating_sub(1));

                                let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                                let new_thumb_start =
                                    pos_in_track.saturating_sub(grab_offset).min(max_start);
                                let new_off = scroll_offset_from_thumb_start(
                                    layout.track_len,
                                    viewport_h,
                                    content_h,
                                    new_thumb_start,
                                );
                                self.inner.set_scroll_offset(scroll_x, new_off);
                                return EventResult::consumed();
                            }
                            ScrollbarDrag::Horizontal { grab_offset } => {
                                let Some(hbar) = hbar else {
                                    self.scrollbar_drag = None;
                                    return EventResult::consumed();
                                };
                                if hbar.width == 0 {
                                    return EventResult::consumed();
                                }

                                let layout = scrollbar_layout_1d(
                                    hbar.width, viewport_w, content_w, scroll_x, cfg.arrows,
                                );
                                if layout.track_len == 0 {
                                    return EventResult::consumed();
                                }

                                let pos = local_x
                                    .saturating_sub(hbar.x)
                                    .min(hbar.width.saturating_sub(1));
                                let pos_in_track = pos
                                    .saturating_sub(layout.track_start)
                                    .min(layout.track_len.saturating_sub(1));

                                let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                                let new_thumb_start =
                                    pos_in_track.saturating_sub(grab_offset).min(max_start);
                                let new_off = scroll_offset_from_thumb_start(
                                    layout.track_len,
                                    viewport_w,
                                    content_w,
                                    new_thumb_start,
                                );
                                self.inner.set_scroll_offset(new_off, scroll_y);
                                return EventResult::consumed();
                            }
                        },
                        MouseEventKind::Up(MouseButton::Left) => {
                            self.scrollbar_drag = None;
                            return EventResult::consumed();
                        }
                        _ => {}
                    }
                }

                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    if let Some(vbar) = vbar
                        && contains(vbar, local_x, local_y)
                        && vbar.height > 0
                    {
                        let pos = local_y.saturating_sub(vbar.y);
                        let layout = scrollbar_layout_1d(
                            vbar.height,
                            viewport_h,
                            content_h,
                            scroll_y,
                            cfg.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                self.inner
                                    .set_scroll_offset(scroll_x, scroll_y.saturating_sub(1));
                                return EventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                self.inner
                                    .set_scroll_offset(scroll_x, scroll_y.saturating_add(1));
                                return EventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag = Some(ScrollbarDrag::Vertical { grab_offset });
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                self.inner.set_scroll_offset(
                                    scroll_x,
                                    scroll_y.saturating_sub(viewport_h),
                                );
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                self.inner.set_scroll_offset(
                                    scroll_x,
                                    scroll_y.saturating_add(viewport_h),
                                );
                                return EventResult::consumed();
                            }
                            ScrollbarHit::None => {}
                        }
                    }

                    if let Some(hbar) = hbar
                        && contains(hbar, local_x, local_y)
                        && hbar.width > 0
                    {
                        let pos = local_x.saturating_sub(hbar.x);
                        let layout = scrollbar_layout_1d(
                            hbar.width, viewport_w, content_w, scroll_x, cfg.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                self.inner
                                    .set_scroll_offset(scroll_x.saturating_sub(1), scroll_y);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                self.inner
                                    .set_scroll_offset(scroll_x.saturating_add(1), scroll_y);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag =
                                    Some(ScrollbarDrag::Horizontal { grab_offset });
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                self.inner.set_scroll_offset(
                                    scroll_x.saturating_sub(viewport_w),
                                    scroll_y,
                                );
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                self.inner.set_scroll_offset(
                                    scroll_x.saturating_add(viewport_w),
                                    scroll_y,
                                );
                                return EventResult::consumed();
                            }
                            ScrollbarHit::None => {}
                        }
                    }
                }
            }

            if !contains(inner_local, local_x, local_y) {
                return EventResult::ignored();
            }

            let child_event = Event::Mouse(MouseEvent {
                column: local_x.saturating_sub(inner_local.x),
                row: local_y.saturating_sub(inner_local.y),
                ..*m
            });
            return self.inner.handle_event(&child_event, inner_ctx);
        }

        self.inner.handle_event(event, inner_ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);

        let border = self.border.get();

        let inner_area = if border { inset(area, 1) } else { area };

        if border && area.width > 0 && area.height > 0 {
            let border_style = ctx.theme.window_bg.patch(if ctx.is_focused {
                ctx.theme.window_border_focused
            } else {
                ctx.theme.window_border
            });
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .border_set(ctx.theme.border_set(false));
            frame.render_widget(block, area);
        }

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        let host_scrollbars = border
            && matches!(ctx.scrollbar_host, ScrollbarHost::Component)
            && self.inner.is_scrollable();
        let inner_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: if host_scrollbars {
                ScrollbarHost::Window
            } else {
                ctx.scrollbar_host
            },
            tab_mode: ctx.tab_mode,
        };

        self.inner.draw(frame, inner_area, inner_ctx);

        if !host_scrollbars {
            return;
        }

        // Border-mounted scrollbars (right + bottom) for scrollable inner views.
        let cfg = self.inner.scroll_config();
        let (content_w, content_h) = self.inner.content_size();
        let (viewport_w, viewport_h) = self.inner.viewport_size();
        let (scroll_x, scroll_y) = self.inner.scroll_offset();

        let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
        let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);

        let local_area = Rect {
            x: 0,
            y: 0,
            width: area.width,
            height: area.height,
        };
        let inner_local = inset(local_area, 1);
        if inner_local.width == 0 || inner_local.height == 0 {
            return;
        }

        let buf = frame.buffer_mut();
        let thumb_style = ctx.theme.window_bg.patch(ctx.theme.scrollbar_thumb);
        let arrow_style = ctx.theme.window_bg.patch(ctx.theme.scrollbar_arrow);

        let thumb = ctx.theme.glyph("scrollbar-thumb").unwrap_or("█");
        let arrow_up = ctx.theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
        let arrow_down = ctx.theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
        let arrow_left = ctx.theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
        let arrow_right = ctx.theme.glyph("scrollbar-right-arrow").unwrap_or("►");

        if show_v && inner_local.height > 0 {
            let layout = scrollbar_layout_1d(
                inner_local.height,
                viewport_h,
                content_h,
                scroll_y,
                cfg.arrows,
            );
            let x = local_area.width.saturating_sub(1);
            for i in 0..inner_local.height {
                let (symbol, style) = if layout.has_arrows && i == 0 {
                    (arrow_up, arrow_style)
                } else if layout.has_arrows && i == layout.bar_len.saturating_sub(1) {
                    (arrow_down, arrow_style)
                } else if i >= layout.thumb_start
                    && i < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    continue;
                };
                buf[(
                    area.x.saturating_add(x),
                    area.y.saturating_add(inner_local.y).saturating_add(i),
                )]
                    .set_symbol(symbol)
                    .set_style(style);
            }
        }

        if show_h && inner_local.width > 0 {
            let layout = scrollbar_layout_1d(
                inner_local.width,
                viewport_w,
                content_w,
                scroll_x,
                cfg.arrows,
            );
            let y = local_area.height.saturating_sub(1);
            for i in 0..inner_local.width {
                let (symbol, style) = if layout.has_arrows && i == 0 {
                    (arrow_left, arrow_style)
                } else if layout.has_arrows && i == layout.bar_len.saturating_sub(1) {
                    (arrow_right, arrow_style)
                } else if i >= layout.thumb_start
                    && i < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    continue;
                };
                buf[(
                    area.x.saturating_add(inner_local.x).saturating_add(i),
                    area.y.saturating_add(y),
                )]
                    .set_symbol(symbol)
                    .set_style(style);
            }
        }
    }
}
