use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::reactive::Binding;
use super::component::{Component, ComponentContext, EventResult, ScrollbarHost};

use super::layout::{EdgeInsets, add_signed, apply_padding};
use super::scroll::{
    ScrollConfig, ScrollOffset, ScrollbarDrag, ScrollbarHit, Scrollbars, clamp_scroll_offset,
    max_scroll_offset, scroll_offset_from_thumb_start, scrollbar_hit_test, scrollbar_layout_1d,
    should_show_scrollbar,
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

/// Public, 1D scrollbar layout info passed to virtual scrolling delegates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollbarLayout {
    pub bar_len: u16,
    pub has_arrows: bool,
    pub track_start: u16,
    pub track_len: u16,
    pub thumb_start: u16,
    pub thumb_len: u16,
}

impl From<super::scroll::ScrollbarLayout1D> for ScrollbarLayout {
    fn from(v: super::scroll::ScrollbarLayout1D) -> Self {
        Self {
            bar_len: v.bar_len,
            has_arrows: v.has_arrows,
            track_start: v.track_start,
            track_len: v.track_len,
            thumb_start: v.thumb_start,
            thumb_len: v.thumb_len,
        }
    }
}

/// Public scrollbar placement passed to virtual scrolling delegates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollbarPlacement {
    pub area: Rect,
    pub layout: ScrollbarLayout,
}

/// Public scrollbar geometry notification (relative to the [`ScrollContainer`] origin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollContainerScrollbars {
    /// Viewport reserved for content (including padding), in ScrollContainer-local coordinates.
    pub viewport: Rect,
    /// Content rect after padding, in ScrollContainer-local coordinates.
    pub content: Rect,
    pub vbar: Option<ScrollbarPlacement>,
    pub hbar: Option<ScrollbarPlacement>,
    pub thickness: u16,
}

/// Context passed to [`ScrollContent`] methods.
#[derive(Clone, Copy, Debug)]
pub struct ScrollContentContext<'a> {
    pub component: ComponentContext<'a>,
    pub info: ScrollContainerInfo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollContainerInfo {
    pub scroll_offset: ScrollOffset,
    pub viewport_size: (u16, u16),
    pub content_size: (u16, u16),
    pub scrollbar_host: ScrollbarHost,
    pub scrollbars: ScrollContainerScrollbars,
}

/// A virtualized content provider for [`ScrollContainer`].
///
/// `ScrollContainer` owns the scroll state + scrollbars and delegates the rendering of the content
/// viewport to a `ScrollContent` implementation.
pub trait ScrollContent: Send {
    fn is_focusable(&self) -> bool {
        false
    }

    fn desired_width(&self) -> Option<u16> {
        None
    }

    fn desired_height(&self) -> Option<u16> {
        None
    }

    /// Returns the virtual content size given the current viewport size.
    ///
    /// The viewport is the content area after padding and after excluding any view-hosted
    /// scrollbars.
    fn content_size(&mut self, viewport: (u16, u16), ctx: ScrollContentContext<'_>) -> (u16, u16);

    /// Called after the outer scroll view computes scrollbar geometry.
    ///
    /// This is the primary mechanism for "scrollbar positioning events".
    fn on_scrollbars(&mut self, _ctx: ScrollContentContext<'_>, _host: &mut ScrollContainerHost) {}

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) -> EventResult {
        EventResult::ignored()
    }

    /// Draw the content viewport.
    ///
    /// `area` is the content area after padding, excluding any borders and scrollbars.
    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        host: &mut ScrollContainerHost,
    );
}

pub struct ScrollContainerHost {
    scroll: Binding<ScrollOffset>,
    content_size: Binding<(u16, u16)>,
    viewport_size: Binding<(u16, u16)>,
}

impl ScrollContainerHost {
    pub fn scroll_offset(&self) -> ScrollOffset {
        self.scroll.get()
    }

    pub fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size.get()
    }

    pub fn content_size(&self) -> (u16, u16) {
        self.content_size.get()
    }

    pub fn set_content_size(&mut self, size: (u16, u16)) {
        self.content_size.set(size);
        self.clamp_scroll();
    }

    pub fn set_scroll_offset(&mut self, x: u16, y: u16) {
        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        self.scroll.set(clamp_scroll_offset(
            content_size,
            viewport_size,
            ScrollOffset { x, y },
        ));
    }

    pub fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        let scroll = self.scroll.get();
        let desired = ScrollOffset {
            x: add_signed(scroll.x, dx),
            y: add_signed(scroll.y, dy),
        };
        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        let clamped = clamp_scroll_offset(content_size, viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

    fn clamp_scroll(&mut self) {
        let scroll = self.scroll.get();
        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        self.scroll
            .set(clamp_scroll_offset(content_size, viewport_size, scroll));
    }
}

pub struct ScrollContainer {
    padding: Binding<EdgeInsets>,
    scroll: Binding<ScrollOffset>,
    content_size: Binding<(u16, u16)>,
    viewport_size: Binding<(u16, u16)>,
    scroll_config: Binding<ScrollConfig>,
    scrollbars: Option<Scrollbars>,
    scrollbar_drag: Option<ScrollbarDrag>,
    last_area: Option<Rect>,
    content: Box<dyn ScrollContent>,
}

impl ScrollContainer {
    pub fn new(content: Box<dyn ScrollContent>) -> Self {
        Self {
            padding: EdgeInsets::ZERO.into(),
            scroll: ScrollOffset::ZERO.into(),
            content_size: (0, 0).into(),
            viewport_size: (0, 0).into(),
            scroll_config: ScrollConfig::default().into(),
            scrollbars: None,
            scrollbar_drag: None,
            last_area: None,
            content,
        }
    }

    pub fn with_padding(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn with_scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.scroll_config = config.into();
        self
    }

    fn info(&self, scrollbar_host: ScrollbarHost) -> ScrollContainerInfo {
        ScrollContainerInfo {
            scroll_offset: self.scroll.get(),
            viewport_size: self.viewport_size.get(),
            content_size: self.content_size.get(),
            scrollbar_host,
            scrollbars: self.scrollbars_info(scrollbar_host),
        }
    }

    fn scrollbars_info(&self, scrollbar_host: ScrollbarHost) -> ScrollContainerScrollbars {
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
                    layout: layout.into(),
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
                    layout: layout.into(),
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

    fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        let desired = ScrollOffset { x, y };
        let scroll = self.scroll.get();
        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        let clamped = clamp_scroll_offset(content_size, viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

    fn handle_event_bubble(&mut self, event: &Event) -> EventResult {
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

    fn draw_scrollbars(&self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
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

        let track_style = ctx.theme.scrollbar_track;
        let thumb_style = ctx.theme.scrollbar_thumb;
        let arrow_style = ctx.theme.scrollbar_arrow;
        let buf = frame.buffer_mut();

        let track = ctx.theme.glyph("scrollbar-track").unwrap_or("░");
        let thumb = ctx.theme.glyph("scrollbar-thumb").unwrap_or("█");
        let arrow_up = ctx.theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
        let arrow_down = ctx.theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
        let arrow_left = ctx.theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
        let arrow_right = ctx.theme.glyph("scrollbar-right-arrow").unwrap_or("►");

        if let Some(vbar) = scrollbars.vbar {
            let layout = scrollbar_layout_1d(
                vbar.height,
                viewport_size.1,
                content_size.1,
                scroll.y,
                cfg.arrows,
            );

            for dy in 0..vbar.height {
                let (symbol, style) = if layout.has_arrows && dy == 0 {
                    (arrow_up, arrow_style)
                } else if layout.has_arrows && dy == layout.bar_len.saturating_sub(1) {
                    (arrow_down, arrow_style)
                } else if dy >= layout.thumb_start
                    && dy < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    (track, track_style)
                };
                for dx in 0..vbar.width {
                    buf[(
                        area.x.saturating_add(vbar.x).saturating_add(dx),
                        area.y.saturating_add(vbar.y).saturating_add(dy),
                    )]
                        .set_symbol(symbol)
                        .set_style(style);
                }
            }
        }

        if let Some(hbar) = scrollbars.hbar {
            let layout = scrollbar_layout_1d(
                hbar.width,
                viewport_size.0,
                content_size.0,
                scroll.x,
                cfg.arrows,
            );

            for dx in 0..hbar.width {
                let (symbol, style) = if layout.has_arrows && dx == 0 {
                    (arrow_left, arrow_style)
                } else if layout.has_arrows && dx == layout.bar_len.saturating_sub(1) {
                    (arrow_right, arrow_style)
                } else if dx >= layout.thumb_start
                    && dx < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    (track, track_style)
                };
                for dy in 0..hbar.height {
                    buf[(
                        area.x.saturating_add(hbar.x).saturating_add(dx),
                        area.y.saturating_add(hbar.y).saturating_add(dy),
                    )]
                        .set_symbol(symbol)
                        .set_style(style);
                }
            }
        }

        if let (Some(vbar), Some(hbar)) = (scrollbars.vbar, scrollbars.hbar) {
            let corner = Rect {
                x: vbar.x,
                y: hbar.y,
                width: vbar.width,
                height: hbar.height,
            };
            for dy in 0..corner.height {
                for dx in 0..corner.width {
                    buf[(
                        area.x.saturating_add(corner.x).saturating_add(dx),
                        area.y.saturating_add(corner.y).saturating_add(dy),
                    )]
                        .set_symbol(track)
                        .set_style(track_style);
                }
            }
        }
    }
}

impl Component for ScrollContainer {
    fn is_focusable(&self) -> bool {
        self.content.is_focusable()
    }

    fn desired_width(&self) -> Option<u16> {
        let w = self.content.desired_width()?;
        Some(w.saturating_add(self.padding.get().sum_horizontal()))
    }

    fn desired_height(&self) -> Option<u16> {
        let h = self.content.desired_height()?;
        Some(h.saturating_add(self.padding.get().sum_vertical()))
    }

    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size.get()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size.get()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config.get()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let scroll = self.scroll.get();
        (scroll.x, scroll.y)
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        let _ = self.scroll_to_clamped(x, y);
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let cfg = self.scroll_config.get();
        let padding = self.padding.get();
        let viewport_size = self.viewport_size.get();
        let content_size = self.content_size.get();
        let scroll = self.scroll.get();
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

            let scrollbars = self.scrollbars.unwrap_or_else(|| {
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
            });

            // If we started a thumb drag, keep consuming drag/up events.
            if let Some(drag) = self.scrollbar_drag {
                match m.kind {
                    crossterm::event::MouseEventKind::Drag(MouseButton::Left) => match drag {
                        ScrollbarDrag::Vertical { grab_offset } => {
                            let Some(vbar) = scrollbars.vbar else {
                                self.scrollbar_drag = None;
                                return EventResult::consumed();
                            };
                            if vbar.height == 0 {
                                return EventResult::consumed();
                            }

                            let layout = scrollbar_layout_1d(
                                vbar.height,
                                viewport_size.1,
                                content_size.1,
                                scroll.y,
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
                                viewport_size.1,
                                content_size.1,
                                new_thumb_start,
                            );
                            let x = self.scroll.get().x;
                            let _ = self.scroll_to_clamped(x, new_off);
                            return EventResult::consumed();
                        }
                        ScrollbarDrag::Horizontal { grab_offset } => {
                            let Some(hbar) = scrollbars.hbar else {
                                self.scrollbar_drag = None;
                                return EventResult::consumed();
                            };
                            if hbar.width == 0 {
                                return EventResult::consumed();
                            }

                            let layout = scrollbar_layout_1d(
                                hbar.width,
                                viewport_size.0,
                                content_size.0,
                                scroll.x,
                                cfg.arrows,
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
                                viewport_size.0,
                                content_size.0,
                                new_thumb_start,
                            );
                            let y = self.scroll.get().y;
                            let _ = self.scroll_to_clamped(new_off, y);
                            return EventResult::consumed();
                        }
                    },
                    crossterm::event::MouseEventKind::Up(MouseButton::Left) => {
                        self.scrollbar_drag = None;
                        return EventResult::consumed();
                    }
                    _ => {}
                }
            }

            if let crossterm::event::MouseEventKind::Down(MouseButton::Left) = m.kind {
                if let Some(vbar) = scrollbars.vbar
                    && contains(vbar, local_x, local_y)
                    && vbar.height > 0
                {
                    let pos = local_y.saturating_sub(vbar.y);
                    let layout = scrollbar_layout_1d(
                        vbar.height,
                        viewport_size.1,
                        content_size.1,
                        scroll.y,
                        cfg.arrows,
                    );
                    match scrollbar_hit_test(layout, pos) {
                        ScrollbarHit::ArrowDec => {
                            let _ = self.scroll_by(0, -1);
                            return EventResult::consumed();
                        }
                        ScrollbarHit::ArrowInc => {
                            let _ = self.scroll_by(0, 1);
                            return EventResult::consumed();
                        }
                        ScrollbarHit::Thumb { grab_offset } => {
                            self.scrollbar_drag = Some(ScrollbarDrag::Vertical { grab_offset });
                            return EventResult::consumed();
                        }
                        ScrollbarHit::TrackDec => {
                            let _ = self.scroll_by(0, -(viewport_size.1 as i16));
                            return EventResult::consumed();
                        }
                        ScrollbarHit::TrackInc => {
                            let _ = self.scroll_by(0, viewport_size.1 as i16);
                            return EventResult::consumed();
                        }
                        ScrollbarHit::None => {}
                    }
                }

                if let Some(hbar) = scrollbars.hbar
                    && contains(hbar, local_x, local_y)
                    && hbar.width > 0
                {
                    let pos = local_x.saturating_sub(hbar.x);
                    let layout = scrollbar_layout_1d(
                        hbar.width,
                        viewport_size.0,
                        content_size.0,
                        scroll.x,
                        cfg.arrows,
                    );
                    match scrollbar_hit_test(layout, pos) {
                        ScrollbarHit::ArrowDec => {
                            let _ = self.scroll_by(-1, 0);
                            return EventResult::consumed();
                        }
                        ScrollbarHit::ArrowInc => {
                            let _ = self.scroll_by(1, 0);
                            return EventResult::consumed();
                        }
                        ScrollbarHit::Thumb { grab_offset } => {
                            self.scrollbar_drag = Some(ScrollbarDrag::Horizontal { grab_offset });
                            return EventResult::consumed();
                        }
                        ScrollbarHit::TrackDec => {
                            let _ = self.scroll_by(-(viewport_size.0 as i16), 0);
                            return EventResult::consumed();
                        }
                        ScrollbarHit::TrackInc => {
                            let _ = self.scroll_by(viewport_size.0 as i16, 0);
                            return EventResult::consumed();
                        }
                        ScrollbarHit::None => {}
                    }
                }
            }
        }

        // Content receives events first.
        if let Event::Mouse(m) = event {
            let Some(area) = self.last_area else {
                return self.handle_event_bubble(event);
            };
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return self.handle_event_bubble(event);
            };

            let scrollbars = self.scrollbars.unwrap_or_else(|| {
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
            });

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

        self.handle_event_bubble(event)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);

        let cfg = self.scroll_config.get();
        let padding = self.padding.get();
        let thickness = cfg.scrollbar_thickness.max(1);

        let mut viewport_outer = area;
        let mut show_v = false;
        let mut show_h = false;

        let child_scrollbar_host = ctx.scrollbar_host.for_child();

        if matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
            // Two-pass solve: scrollbar visibility affects viewport size (which can affect
            // content size for virtualized content).
            for _ in 0..2 {
                let inner = apply_padding(viewport_outer, padding);
                self.viewport_size.set((inner.width, inner.height));

                let info = self.info(ctx.scrollbar_host);
                let content_ctx = ScrollContentContext {
                    component: ComponentContext {
                        theme: ctx.theme,
                        window_id: ctx.window_id,
                        is_focused: ctx.is_focused,
                        scrollbar_host: child_scrollbar_host,
                        tab_mode: ctx.tab_mode,
                    },
                    info,
                };
                let viewport_size = self.viewport_size.get();
                let new_content_size = self.content.content_size(viewport_size, content_ctx);
                self.content_size.set(new_content_size);

                let viewport_size = self.viewport_size.get();
                let content_size = self.content_size.get();
                let new_show_v =
                    should_show_scrollbar(cfg.vertical_scrollbar, content_size.1, viewport_size.1);
                let new_show_h = should_show_scrollbar(
                    cfg.horizontal_scrollbar,
                    content_size.0,
                    viewport_size.0,
                );

                if new_show_v == show_v && new_show_h == show_h {
                    break;
                }

                show_v = new_show_v;
                show_h = new_show_h;

                let v_thick = if show_v { thickness } else { 0 };
                let h_thick = if show_h { thickness } else { 0 };
                viewport_outer = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width.saturating_sub(v_thick),
                    height: area.height.saturating_sub(h_thick),
                };
            }
        } else {
            let inner = apply_padding(area, padding);
            self.viewport_size.set((inner.width, inner.height));

            let info = self.info(ctx.scrollbar_host);
            let content_ctx = ScrollContentContext {
                component: ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: child_scrollbar_host,
                    tab_mode: ctx.tab_mode,
                },
                info,
            };
            let viewport_size = self.viewport_size.get();
            self.content_size
                .set(self.content.content_size(viewport_size, content_ctx));

            let viewport_size = self.viewport_size.get();
            let content_size = self.content_size.get();
            show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_size.1, viewport_size.1);
            show_h =
                should_show_scrollbar(cfg.horizontal_scrollbar, content_size.0, viewport_size.0);
        }

        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        let scroll = self.scroll.get();
        self.scroll
            .set(clamp_scroll_offset(content_size, viewport_size, scroll));

        if matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
            let viewport_local = Rect {
                x: viewport_outer.x.saturating_sub(area.x),
                y: viewport_outer.y.saturating_sub(area.y),
                width: viewport_outer.width,
                height: viewport_outer.height,
            };
            let content_local = apply_padding(viewport_local, padding);
            let vbar = show_v.then_some(Rect {
                x: viewport_local.x.saturating_add(viewport_local.width),
                y: viewport_local.y,
                width: thickness,
                height: viewport_local.height,
            });
            let hbar = show_h.then_some(Rect {
                x: viewport_local.x,
                y: viewport_local.y.saturating_add(viewport_local.height),
                width: viewport_local.width,
                height: thickness,
            });
            self.scrollbars = Some(Scrollbars {
                viewport: viewport_local,
                content: content_local,
                vbar,
                hbar,
                thickness,
            });
            if !show_v && !show_h {
                self.scrollbar_drag = None;
            }
        } else {
            self.scrollbars = None;
            self.scrollbar_drag = None;
        }

        // Notify the delegate of geometry + allow it to adjust host state.
        let info = self.info(ctx.scrollbar_host);
        let content_ctx = ScrollContentContext {
            component: ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: ctx.is_focused,
                scrollbar_host: child_scrollbar_host,
                tab_mode: ctx.tab_mode,
            },
            info,
        };
        let mut host = ScrollContainerHost {
            scroll: self.scroll.clone(),
            content_size: self.content_size.clone(),
            viewport_size: self.viewport_size.clone(),
        };
        self.content.on_scrollbars(content_ctx, &mut host);

        // Clamp again in case the delegate updated content size or scroll offset.
        let content_size = self.content_size.get();
        let viewport_size = self.viewport_size.get();
        let scroll = self.scroll.get();
        self.scroll
            .set(clamp_scroll_offset(content_size, viewport_size, scroll));

        // Draw the content viewport (after padding, excluding scrollbars).
        let content_area = apply_padding(viewport_outer, padding);
        if content_area.width > 0 && content_area.height > 0 {
            let info = self.info(ctx.scrollbar_host);
            let content_ctx = ScrollContentContext {
                component: ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: child_scrollbar_host,
                    tab_mode: ctx.tab_mode,
                },
                info,
            };
            let mut host = ScrollContainerHost {
                scroll: self.scroll.clone(),
                content_size: self.content_size.clone(),
                viewport_size: self.viewport_size.clone(),
            };
            self.content
                .draw(frame, content_area, content_ctx, &mut host);
        }

        self.draw_scrollbars(frame, area, ctx);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::theme::Theme;
    use crate::composable::{ComponentContext, ScrollbarHost, TabMode};
    use crate::wm::WindowId;

    use super::*;

    #[derive(Clone)]
    struct RecordingContent {
        last_area: Arc<Mutex<Option<Rect>>>,
    }

    impl ScrollContent for RecordingContent {
        fn content_size(
            &mut self,
            _viewport: (u16, u16),
            ctx: ScrollContentContext<'_>,
        ) -> (u16, u16) {
            // Always force both scrollbars on in the test.
            (
                ctx.info.viewport_size.0.saturating_add(10),
                ctx.info.viewport_size.1.saturating_add(10),
            )
        }

        fn draw(
            &mut self,
            _frame: &mut Frame<'_>,
            area: Rect,
            _ctx: ScrollContentContext<'_>,
            _host: &mut ScrollContainerHost,
        ) {
            *self.last_area.lock().expect("lock") = Some(area);
        }
    }

    #[test]
    fn content_area_excludes_view_hosted_scrollbars() {
        let recorded = Arc::new(Mutex::new(None));
        let content = RecordingContent {
            last_area: Arc::clone(&recorded),
        };
        let mut view = ScrollContainer::new(Box::new(content)).with_scroll_config(
            ScrollConfig::default()
                .vertical_scrollbar(super::super::scroll::ScrollbarVisibility::Always)
                .horizontal_scrollbar(super::super::scroll::ScrollbarVisibility::Always),
        );

        let theme = Theme::dark();
        let ctx = ComponentContext {
            theme: &theme,
            window_id: WindowId(1),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
        };

        let backend = TestBackend::new(10, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| view.draw(f, Rect::new(0, 0, 10, 10), ctx))
            .expect("draw");

        let area = recorded.lock().expect("lock").expect("area recorded");
        assert_eq!(area.width, 9);
        assert_eq!(area.height, 9);
    }
}
