use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::{ScrollbarHost, View, ViewContext, ViewEventResult};

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

/// Public scrollbar geometry notification (relative to the [`ScrollView`] origin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollViewScrollbars {
    /// Viewport reserved for content (including padding), in ScrollView-local coordinates.
    pub viewport: Rect,
    /// Content rect after padding, in ScrollView-local coordinates.
    pub content: Rect,
    pub vbar: Option<ScrollbarPlacement>,
    pub hbar: Option<ScrollbarPlacement>,
    pub thickness: u16,
}

/// Context passed to [`ScrollContent`] methods.
#[derive(Clone, Copy, Debug)]
pub struct ScrollContentContext<'a> {
    pub view: ViewContext<'a>,
    pub info: ScrollViewInfo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollViewInfo {
    pub scroll_offset: ScrollOffset,
    pub viewport_size: (u16, u16),
    pub content_size: (u16, u16),
    pub scrollbar_host: ScrollbarHost,
    pub scrollbars: ScrollViewScrollbars,
}

/// A virtualized content provider for [`ScrollView`].
///
/// `ScrollView` owns the scroll state + scrollbars and delegates the rendering of the content
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
    fn on_scrollbars(&mut self, _ctx: ScrollContentContext<'_>, _host: &mut ScrollViewHost<'_>) {}

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: ScrollContentContext<'_>,
        _host: &mut ScrollViewHost<'_>,
    ) -> ViewEventResult {
        ViewEventResult::ignored()
    }

    /// Draw the content viewport.
    ///
    /// `area` is the content area after padding, excluding any borders and scrollbars.
    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        host: &mut ScrollViewHost<'_>,
    );
}

pub struct ScrollViewHost<'a> {
    scroll: &'a mut ScrollOffset,
    content_size: &'a mut (u16, u16),
    viewport_size: &'a mut (u16, u16),
}

impl<'a> ScrollViewHost<'a> {
    pub fn scroll_offset(&self) -> ScrollOffset {
        *self.scroll
    }

    pub fn viewport_size(&self) -> (u16, u16) {
        *self.viewport_size
    }

    pub fn content_size(&self) -> (u16, u16) {
        *self.content_size
    }

    pub fn set_content_size(&mut self, size: (u16, u16)) {
        *self.content_size = size;
        self.clamp_scroll();
    }

    pub fn set_scroll_offset(&mut self, x: u16, y: u16) {
        *self.scroll = clamp_scroll_offset(
            *self.content_size,
            *self.viewport_size,
            ScrollOffset { x, y },
        );
    }

    pub fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        let desired = ScrollOffset {
            x: add_signed(self.scroll.x, dx),
            y: add_signed(self.scroll.y, dy),
        };
        let clamped = clamp_scroll_offset(*self.content_size, *self.viewport_size, desired);
        let changed = clamped != *self.scroll;
        *self.scroll = clamped;
        changed
    }

    fn clamp_scroll(&mut self) {
        *self.scroll = clamp_scroll_offset(*self.content_size, *self.viewport_size, *self.scroll);
    }
}

pub struct ScrollView {
    padding: EdgeInsets,
    scroll: ScrollOffset,
    content_size: (u16, u16),
    viewport_size: (u16, u16),
    scroll_config: ScrollConfig,
    scrollbars: Option<Scrollbars>,
    scrollbar_drag: Option<ScrollbarDrag>,
    last_area: Option<Rect>,
    content: Box<dyn ScrollContent>,
}

impl ScrollView {
    pub fn new(content: Box<dyn ScrollContent>) -> Self {
        Self {
            padding: EdgeInsets::ZERO,
            scroll: ScrollOffset::ZERO,
            content_size: (0, 0),
            viewport_size: (0, 0),
            scroll_config: ScrollConfig::default(),
            scrollbars: None,
            scrollbar_drag: None,
            last_area: None,
            content,
        }
    }

    pub fn with_padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn with_scroll_config(mut self, config: ScrollConfig) -> Self {
        self.scroll_config = config;
        self
    }

    fn info(&self, scrollbar_host: ScrollbarHost) -> ScrollViewInfo {
        ScrollViewInfo {
            scroll_offset: self.scroll,
            viewport_size: self.viewport_size,
            content_size: self.content_size,
            scrollbar_host,
            scrollbars: self.scrollbars_info(scrollbar_host),
        }
    }

    fn scrollbars_info(&self, scrollbar_host: ScrollbarHost) -> ScrollViewScrollbars {
        let thickness = self.scroll_config.scrollbar_thickness.max(1);
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
                content: apply_padding(viewport, self.padding),
                vbar: None,
                hbar: None,
                thickness,
            }
        } else {
            return ScrollViewScrollbars::default();
        };

        let vbar = (matches!(scrollbar_host, ScrollbarHost::View))
            .then_some(scrollbars.vbar)
            .flatten()
            .map(|area| {
                let layout = scrollbar_layout_1d(
                    area.height,
                    self.viewport_size.1,
                    self.content_size.1,
                    self.scroll.y,
                    self.scroll_config.arrows,
                );
                ScrollbarPlacement {
                    area,
                    layout: layout.into(),
                }
            });
        let hbar = (matches!(scrollbar_host, ScrollbarHost::View))
            .then_some(scrollbars.hbar)
            .flatten()
            .map(|area| {
                let layout = scrollbar_layout_1d(
                    area.width,
                    self.viewport_size.0,
                    self.content_size.0,
                    self.scroll.x,
                    self.scroll_config.arrows,
                );
                ScrollbarPlacement {
                    area,
                    layout: layout.into(),
                }
            });

        ScrollViewScrollbars {
            viewport: scrollbars.viewport,
            content: scrollbars.content,
            vbar,
            hbar,
            thickness: scrollbars.thickness,
        }
    }

    fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        let desired = ScrollOffset {
            x: add_signed(self.scroll.x, dx),
            y: add_signed(self.scroll.y, dy),
        };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != self.scroll;
        self.scroll = clamped;
        changed
    }

    fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        let desired = ScrollOffset { x, y };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != self.scroll;
        self.scroll = clamped;
        changed
    }

    fn handle_event_bubble(&mut self, event: &Event) -> ViewEventResult {
        match event {
            Event::Key(KeyEvent { code, kind, .. }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return ViewEventResult::ignored();
                }

                let viewport_h = self.viewport_size.1;
                let max = max_scroll_offset(self.content_size, self.viewport_size);

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
                    ViewEventResult::consumed()
                } else {
                    ViewEventResult::ignored()
                }
            }
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return ViewEventResult::ignored();
                };
                if mouse_coords_local_to_area(area, *m).is_none() {
                    return ViewEventResult::ignored();
                }

                let step = self.scroll_config.wheel_step as i16;
                let changed = match m.kind {
                    crossterm::event::MouseEventKind::ScrollUp => self.scroll_by(0, -step),
                    crossterm::event::MouseEventKind::ScrollDown => self.scroll_by(0, step),
                    crossterm::event::MouseEventKind::ScrollLeft => self.scroll_by(-step, 0),
                    crossterm::event::MouseEventKind::ScrollRight => self.scroll_by(step, 0),
                    _ => false,
                };

                if changed {
                    ViewEventResult::consumed()
                } else {
                    ViewEventResult::ignored()
                }
            }
            _ => ViewEventResult::ignored(),
        }
    }

    fn draw_scrollbars(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let Some(scrollbars) = self.scrollbars else {
            return;
        };

        if !matches!(ctx.scrollbar_host, ScrollbarHost::View) {
            return;
        }

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
                self.viewport_size.1,
                self.content_size.1,
                self.scroll.y,
                self.scroll_config.arrows,
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
                self.viewport_size.0,
                self.content_size.0,
                self.scroll.x,
                self.scroll_config.arrows,
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

impl View for ScrollView {
    fn is_focusable(&self) -> bool {
        self.content.is_focusable()
    }

    fn desired_width(&self) -> Option<u16> {
        let w = self.content.desired_width()?;
        Some(w.saturating_add(self.padding.sum_horizontal()))
    }

    fn desired_height(&self) -> Option<u16> {
        let h = self.content.desired_height()?;
        Some(h.saturating_add(self.padding.sum_vertical()))
    }

    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config
    }

    fn scroll_offset(&self) -> (u16, u16) {
        (self.scroll.x, self.scroll.y)
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        let _ = self.scroll_to_clamped(x, y);
    }

    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        let info = self.info(ctx.scrollbar_host);
        let content_ctx = ScrollContentContext {
            view: ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: ctx.is_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
            },
            info,
        };

        // Scrollbar hit-testing is only relevant when scrollbars are hosted by the view itself.
        if matches!(ctx.scrollbar_host, ScrollbarHost::View)
            && let Event::Mouse(m) = event
        {
            let Some(area) = self.last_area else {
                return ViewEventResult::ignored();
            };
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return ViewEventResult::ignored();
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
                    content: apply_padding(viewport, self.padding),
                    vbar: None,
                    hbar: None,
                    thickness: self.scroll_config.scrollbar_thickness.max(1),
                }
            });

            // If we started a thumb drag, keep consuming drag/up events.
            if let Some(drag) = self.scrollbar_drag {
                match m.kind {
                    crossterm::event::MouseEventKind::Drag(MouseButton::Left) => match drag {
                        ScrollbarDrag::Vertical { grab_offset } => {
                            let Some(vbar) = scrollbars.vbar else {
                                self.scrollbar_drag = None;
                                return ViewEventResult::consumed();
                            };
                            if vbar.height == 0 {
                                return ViewEventResult::consumed();
                            }

                            let layout = scrollbar_layout_1d(
                                vbar.height,
                                self.viewport_size.1,
                                self.content_size.1,
                                self.scroll.y,
                                self.scroll_config.arrows,
                            );
                            if layout.track_len == 0 {
                                return ViewEventResult::consumed();
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
                                self.viewport_size.1,
                                self.content_size.1,
                                new_thumb_start,
                            );
                            let _ = self.scroll_to_clamped(self.scroll.x, new_off);
                            return ViewEventResult::consumed();
                        }
                        ScrollbarDrag::Horizontal { grab_offset } => {
                            let Some(hbar) = scrollbars.hbar else {
                                self.scrollbar_drag = None;
                                return ViewEventResult::consumed();
                            };
                            if hbar.width == 0 {
                                return ViewEventResult::consumed();
                            }

                            let layout = scrollbar_layout_1d(
                                hbar.width,
                                self.viewport_size.0,
                                self.content_size.0,
                                self.scroll.x,
                                self.scroll_config.arrows,
                            );
                            if layout.track_len == 0 {
                                return ViewEventResult::consumed();
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
                                self.viewport_size.0,
                                self.content_size.0,
                                new_thumb_start,
                            );
                            let _ = self.scroll_to_clamped(new_off, self.scroll.y);
                            return ViewEventResult::consumed();
                        }
                    },
                    crossterm::event::MouseEventKind::Up(MouseButton::Left) => {
                        self.scrollbar_drag = None;
                        return ViewEventResult::consumed();
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
                        self.viewport_size.1,
                        self.content_size.1,
                        self.scroll.y,
                        self.scroll_config.arrows,
                    );
                    match scrollbar_hit_test(layout, pos) {
                        ScrollbarHit::ArrowDec => {
                            let _ = self.scroll_by(0, -1);
                            return ViewEventResult::consumed();
                        }
                        ScrollbarHit::ArrowInc => {
                            let _ = self.scroll_by(0, 1);
                            return ViewEventResult::consumed();
                        }
                        ScrollbarHit::Thumb { grab_offset } => {
                            self.scrollbar_drag = Some(ScrollbarDrag::Vertical { grab_offset });
                            return ViewEventResult::consumed();
                        }
                        ScrollbarHit::TrackDec => {
                            let _ = self.scroll_by(0, -(self.viewport_size.1 as i16));
                            return ViewEventResult::consumed();
                        }
                        ScrollbarHit::TrackInc => {
                            let _ = self.scroll_by(0, self.viewport_size.1 as i16);
                            return ViewEventResult::consumed();
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
                        self.viewport_size.0,
                        self.content_size.0,
                        self.scroll.x,
                        self.scroll_config.arrows,
                    );
                    match scrollbar_hit_test(layout, pos) {
                        ScrollbarHit::ArrowDec => {
                            let _ = self.scroll_by(-1, 0);
                            return ViewEventResult::consumed();
                        }
                        ScrollbarHit::ArrowInc => {
                            let _ = self.scroll_by(1, 0);
                            return ViewEventResult::consumed();
                        }
                        ScrollbarHit::Thumb { grab_offset } => {
                            self.scrollbar_drag = Some(ScrollbarDrag::Horizontal { grab_offset });
                            return ViewEventResult::consumed();
                        }
                        ScrollbarHit::TrackDec => {
                            let _ = self.scroll_by(-(self.viewport_size.0 as i16), 0);
                            return ViewEventResult::consumed();
                        }
                        ScrollbarHit::TrackInc => {
                            let _ = self.scroll_by(self.viewport_size.0 as i16, 0);
                            return ViewEventResult::consumed();
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
                    content: apply_padding(viewport, self.padding),
                    vbar: None,
                    hbar: None,
                    thickness: self.scroll_config.scrollbar_thickness.max(1),
                }
            });

            let content = scrollbars.content;
            if contains(content, local_x, local_y) {
                let child_event = Event::Mouse(MouseEvent {
                    column: local_x.saturating_sub(content.x),
                    row: local_y.saturating_sub(content.y),
                    ..*m
                });

                let mut host = ScrollViewHost {
                    scroll: &mut self.scroll,
                    content_size: &mut self.content_size,
                    viewport_size: &mut self.viewport_size,
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
            let mut host = ScrollViewHost {
                scroll: &mut self.scroll,
                content_size: &mut self.content_size,
                viewport_size: &mut self.viewport_size,
            };
            self.content.on_scrollbars(content_ctx, &mut host);
            let res = self.content.handle_event(event, content_ctx, &mut host);
            if res.is_consumed() {
                return res;
            }
        }

        self.handle_event_bubble(event)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.last_area = Some(area);

        let thickness = self.scroll_config.scrollbar_thickness.max(1);

        let mut viewport_outer = area;
        let mut show_v = false;
        let mut show_h = false;

        let child_scrollbar_host = ctx.scrollbar_host.for_child();

        if matches!(ctx.scrollbar_host, ScrollbarHost::View) {
            // Two-pass solve: scrollbar visibility affects viewport size (which can affect
            // content size for virtualized content).
            for _ in 0..2 {
                let inner = apply_padding(viewport_outer, self.padding);
                self.viewport_size = (inner.width, inner.height);

                let info = self.info(ctx.scrollbar_host);
                let content_ctx = ScrollContentContext {
                    view: ViewContext {
                        theme: ctx.theme,
                        window_id: ctx.window_id,
                        is_focused: ctx.is_focused,
                        scrollbar_host: child_scrollbar_host,
                    },
                    info,
                };
                let new_content_size = self.content.content_size(self.viewport_size, content_ctx);
                self.content_size = new_content_size;

                let new_show_v = should_show_scrollbar(
                    self.scroll_config.vertical_scrollbar,
                    self.content_size.1,
                    self.viewport_size.1,
                );
                let new_show_h = should_show_scrollbar(
                    self.scroll_config.horizontal_scrollbar,
                    self.content_size.0,
                    self.viewport_size.0,
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
            let inner = apply_padding(area, self.padding);
            self.viewport_size = (inner.width, inner.height);

            let info = self.info(ctx.scrollbar_host);
            let content_ctx = ScrollContentContext {
                view: ViewContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: child_scrollbar_host,
                },
                info,
            };
            self.content_size = self.content.content_size(self.viewport_size, content_ctx);

            show_v = should_show_scrollbar(
                self.scroll_config.vertical_scrollbar,
                self.content_size.1,
                self.viewport_size.1,
            );
            show_h = should_show_scrollbar(
                self.scroll_config.horizontal_scrollbar,
                self.content_size.0,
                self.viewport_size.0,
            );
        }

        self.scroll = clamp_scroll_offset(self.content_size, self.viewport_size, self.scroll);

        if matches!(ctx.scrollbar_host, ScrollbarHost::View) {
            let viewport_local = Rect {
                x: viewport_outer.x.saturating_sub(area.x),
                y: viewport_outer.y.saturating_sub(area.y),
                width: viewport_outer.width,
                height: viewport_outer.height,
            };
            let content_local = apply_padding(viewport_local, self.padding);
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
            view: ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: ctx.is_focused,
                scrollbar_host: child_scrollbar_host,
            },
            info,
        };
        let mut host = ScrollViewHost {
            scroll: &mut self.scroll,
            content_size: &mut self.content_size,
            viewport_size: &mut self.viewport_size,
        };
        self.content.on_scrollbars(content_ctx, &mut host);

        // Clamp again in case the delegate updated content size or scroll offset.
        self.scroll = clamp_scroll_offset(self.content_size, self.viewport_size, self.scroll);

        // Draw the content viewport (after padding, excluding scrollbars).
        let content_area = apply_padding(viewport_outer, self.padding);
        if content_area.width > 0 && content_area.height > 0 {
            let info = self.info(ctx.scrollbar_host);
            let content_ctx = ScrollContentContext {
                view: ViewContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: child_scrollbar_host,
                },
                info,
            };
            let mut host = ScrollViewHost {
                scroll: &mut self.scroll,
                content_size: &mut self.content_size,
                viewport_size: &mut self.viewport_size,
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
    use crate::view::{ScrollbarHost, ViewContext};
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
            _host: &mut ScrollViewHost<'_>,
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
        let mut view = ScrollView::new(Box::new(content)).with_scroll_config(
            ScrollConfig::default()
                .vertical_scrollbar(super::super::scroll::ScrollbarVisibility::Always)
                .horizontal_scrollbar(super::super::scroll::ScrollbarVisibility::Always),
        );

        let theme = Theme::dark();
        let ctx = ViewContext {
            theme: &theme,
            window_id: WindowId(1),
            is_focused: true,
            scrollbar_host: ScrollbarHost::View,
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
