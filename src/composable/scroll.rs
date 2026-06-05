use std::cmp;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;

use super::geom::contains;
use super::layout::{EdgeInsets, add_signed, apply_padding};
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollOffset {
    pub x: u16,
    pub y: u16,
}

impl ScrollOffset {
    pub const ZERO: Self = Self { x: 0, y: 0 };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollConfig {
    /// Mouse wheel step in rows/cols (default: 3).
    pub wheel_step: u16,

    /// Scrollbar thickness in terminal cells (default: 1).
    pub scrollbar_thickness: u16,

    pub vertical_scrollbar: ScrollbarVisibility,
    pub horizontal_scrollbar: ScrollbarVisibility,

    /// Whether scrollbars include arrow buttons at the ends.
    pub arrows: bool,
}

impl ScrollConfig {
    pub const fn wheel_step(mut self, step: u16) -> Self {
        self.wheel_step = step;
        self
    }

    pub const fn scrollbar_thickness(mut self, thickness: u16) -> Self {
        self.scrollbar_thickness = thickness;
        self
    }

    pub const fn vertical_scrollbar(mut self, vis: ScrollbarVisibility) -> Self {
        self.vertical_scrollbar = vis;
        self
    }

    pub const fn horizontal_scrollbar(mut self, vis: ScrollbarVisibility) -> Self {
        self.horizontal_scrollbar = vis;
        self
    }

    pub const fn arrows(mut self, enabled: bool) -> Self {
        self.arrows = enabled;
        self
    }
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            wheel_step: 3,
            scrollbar_thickness: 1,
            vertical_scrollbar: ScrollbarVisibility::Auto,
            horizontal_scrollbar: ScrollbarVisibility::Auto,
            arrows: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarVisibility {
    Always,
    #[default]
    Auto,
    Never,
}

/// Concrete scrollbar geometry for a scrollable view.
///
/// This is used by view-hosted scrollbars as well as by higher-level components that want to
/// "mount" a child's scrollbars onto a parent border (e.g. split panes, bordered widgets).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scrollbars {
    /// Viewport area (relative to the view's origin) that is reserved for content (including padding).
    pub viewport: Rect,
    /// Content area (relative to the view's origin) after padding is applied.
    pub content: Rect,
    /// Vertical scrollbar area (relative), if visible.
    pub vbar: Option<Rect>,
    /// Horizontal scrollbar area (relative), if visible.
    pub hbar: Option<Rect>,
    pub thickness: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarDrag {
    Vertical { grab_offset: u16 },
    Horizontal { grab_offset: u16 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ResolvedScrollView {
    pub inner: Rect,
    pub viewport_size: (u16, u16),
    pub content_size: (u16, u16),
    pub scrollbars: Option<Scrollbars>,
    pub show_v: bool,
    pub show_h: bool,
}

pub(crate) fn scrollbars_for_event(
    area: Rect,
    padding: EdgeInsets,
    thickness: u16,
    existing: Option<Scrollbars>,
) -> Scrollbars {
    if let Some(scrollbars) = existing {
        return scrollbars;
    }

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
        thickness: thickness.max(1),
    }
}

pub(crate) fn scroll_by_delta(
    content_size: (u16, u16),
    viewport_size: (u16, u16),
    scroll: ScrollOffset,
    dx: i16,
    dy: i16,
) -> ScrollOffset {
    clamp_scroll_offset(
        content_size,
        viewport_size,
        ScrollOffset {
            x: add_signed(scroll.x, dx),
            y: add_signed(scroll.y, dy),
        },
    )
}

pub(crate) fn scroll_offset_for_input_event(
    cfg: ScrollConfig,
    content_size: (u16, u16),
    viewport_size: (u16, u16),
    scroll: ScrollOffset,
    event: &Event,
) -> Option<ScrollOffset> {
    match event {
        Event::Key(KeyEvent { code, kind, .. }) => {
            if matches!(kind, KeyEventKind::Release) {
                return None;
            }

            let next = match code {
                KeyCode::Up => scroll_by_delta(content_size, viewport_size, scroll, 0, -1),
                KeyCode::Down => scroll_by_delta(content_size, viewport_size, scroll, 0, 1),
                KeyCode::Left => scroll_by_delta(content_size, viewport_size, scroll, -1, 0),
                KeyCode::Right => scroll_by_delta(content_size, viewport_size, scroll, 1, 0),
                KeyCode::PageUp => scroll_by_delta(
                    content_size,
                    viewport_size,
                    scroll,
                    0,
                    -(viewport_size.1 as i16),
                ),
                KeyCode::PageDown => scroll_by_delta(
                    content_size,
                    viewport_size,
                    scroll,
                    0,
                    viewport_size.1 as i16,
                ),
                KeyCode::Home => ScrollOffset::ZERO,
                KeyCode::End => max_scroll_offset(content_size, viewport_size),
                _ => return None,
            };
            Some(next)
        }
        Event::Mouse(m) => {
            let step = cfg.wheel_step as i16;
            let next = match m.kind {
                MouseEventKind::ScrollUp => {
                    scroll_by_delta(content_size, viewport_size, scroll, 0, -step)
                }
                MouseEventKind::ScrollDown => {
                    scroll_by_delta(content_size, viewport_size, scroll, 0, step)
                }
                MouseEventKind::ScrollLeft => {
                    scroll_by_delta(content_size, viewport_size, scroll, -step, 0)
                }
                MouseEventKind::ScrollRight => {
                    scroll_by_delta(content_size, viewport_size, scroll, step, 0)
                }
                _ => return None,
            };
            Some(next)
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn handle_scrollbar_mouse_event(
    cfg: ScrollConfig,
    scrollbars: Scrollbars,
    content_size: (u16, u16),
    scroll: ScrollOffset,
    drag: &mut Option<ScrollbarDrag>,
    local_x: u16,
    local_y: u16,
    kind: MouseEventKind,
) -> Option<ScrollOffset> {
    let viewport_size = (scrollbars.content.width, scrollbars.content.height);

    if let Some(active) = *drag {
        match kind {
            MouseEventKind::Drag(MouseButton::Left) => match active {
                ScrollbarDrag::Vertical { grab_offset } => {
                    let Some(vbar) = scrollbars.vbar else {
                        *drag = None;
                        return Some(scroll);
                    };
                    if vbar.height == 0 {
                        return Some(scroll);
                    }

                    let layout = scrollbar_layout_1d(
                        vbar.height,
                        viewport_size.1,
                        content_size.1,
                        scroll.y,
                        cfg.arrows,
                    );
                    if layout.track_len == 0 {
                        return Some(scroll);
                    }

                    let pos = local_y
                        .saturating_sub(vbar.y)
                        .min(vbar.height.saturating_sub(1));
                    let pos_in_track = pos
                        .saturating_sub(layout.track_start)
                        .min(layout.track_len.saturating_sub(1));

                    let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                    let new_thumb_start = pos_in_track.saturating_sub(grab_offset).min(max_start);
                    let new_off = scroll_offset_from_thumb_start(
                        layout.track_len,
                        viewport_size.1,
                        content_size.1,
                        new_thumb_start,
                    );
                    return Some(ScrollOffset {
                        x: scroll.x,
                        y: new_off,
                    });
                }
                ScrollbarDrag::Horizontal { grab_offset } => {
                    let Some(hbar) = scrollbars.hbar else {
                        *drag = None;
                        return Some(scroll);
                    };
                    if hbar.width == 0 {
                        return Some(scroll);
                    }

                    let layout = scrollbar_layout_1d(
                        hbar.width,
                        viewport_size.0,
                        content_size.0,
                        scroll.x,
                        cfg.arrows,
                    );
                    if layout.track_len == 0 {
                        return Some(scroll);
                    }

                    let pos = local_x
                        .saturating_sub(hbar.x)
                        .min(hbar.width.saturating_sub(1));
                    let pos_in_track = pos
                        .saturating_sub(layout.track_start)
                        .min(layout.track_len.saturating_sub(1));

                    let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                    let new_thumb_start = pos_in_track.saturating_sub(grab_offset).min(max_start);
                    let new_off = scroll_offset_from_thumb_start(
                        layout.track_len,
                        viewport_size.0,
                        content_size.0,
                        new_thumb_start,
                    );
                    return Some(ScrollOffset {
                        x: new_off,
                        y: scroll.y,
                    });
                }
            },
            MouseEventKind::Up(MouseButton::Left) => {
                *drag = None;
                return Some(scroll);
            }
            _ => {}
        }
    }

    if let MouseEventKind::Down(MouseButton::Left) = kind {
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
                    return Some(scroll_by_delta(content_size, viewport_size, scroll, 0, -1));
                }
                ScrollbarHit::ArrowInc => {
                    return Some(scroll_by_delta(content_size, viewport_size, scroll, 0, 1));
                }
                ScrollbarHit::Thumb { grab_offset } => {
                    *drag = Some(ScrollbarDrag::Vertical { grab_offset });
                    return Some(scroll);
                }
                ScrollbarHit::TrackDec => {
                    let page = viewport_size.1 as i16;
                    return Some(scroll_by_delta(
                        content_size,
                        viewport_size,
                        scroll,
                        0,
                        -page,
                    ));
                }
                ScrollbarHit::TrackInc => {
                    let page = viewport_size.1 as i16;
                    return Some(scroll_by_delta(
                        content_size,
                        viewport_size,
                        scroll,
                        0,
                        page,
                    ));
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
                    return Some(scroll_by_delta(content_size, viewport_size, scroll, -1, 0));
                }
                ScrollbarHit::ArrowInc => {
                    return Some(scroll_by_delta(content_size, viewport_size, scroll, 1, 0));
                }
                ScrollbarHit::Thumb { grab_offset } => {
                    *drag = Some(ScrollbarDrag::Horizontal { grab_offset });
                    return Some(scroll);
                }
                ScrollbarHit::TrackDec => {
                    let page = viewport_size.0 as i16;
                    return Some(scroll_by_delta(
                        content_size,
                        viewport_size,
                        scroll,
                        -page,
                        0,
                    ));
                }
                ScrollbarHit::TrackInc => {
                    let page = viewport_size.0 as i16;
                    return Some(scroll_by_delta(
                        content_size,
                        viewport_size,
                        scroll,
                        page,
                        0,
                    ));
                }
                ScrollbarHit::None => {}
            }
        }
    }

    None
}

pub(crate) fn resolve_scroll_view(
    area: Rect,
    padding: EdgeInsets,
    cfg: ScrollConfig,
    scrollable: bool,
    host_scrollbars: bool,
    mut content_size_for_viewport: impl FnMut((u16, u16)) -> (u16, u16),
) -> ResolvedScrollView {
    let thickness = cfg.scrollbar_thickness.max(1);

    let mut viewport_outer = area;
    let mut inner = apply_padding(viewport_outer, padding);
    let mut viewport_size = (inner.width, inner.height);
    let mut content_size = (0, 0);

    let mut show_v = false;
    let mut show_h = false;

    if scrollable && host_scrollbars {
        // Two-pass solve: scrollbar visibility affects viewport size (which affects content size).
        for _ in 0..2 {
            inner = apply_padding(viewport_outer, padding);
            viewport_size = (inner.width, inner.height);
            content_size = content_size_for_viewport(viewport_size);

            let new_show_v =
                should_show_scrollbar(cfg.vertical_scrollbar, content_size.1, viewport_size.1);
            let new_show_h =
                should_show_scrollbar(cfg.horizontal_scrollbar, content_size.0, viewport_size.0);

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
        viewport_outer = area;
        inner = apply_padding(area, padding);
        viewport_size = (inner.width, inner.height);
        content_size = content_size_for_viewport(viewport_size);

        if scrollable {
            show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_size.1, viewport_size.1);
            show_h =
                should_show_scrollbar(cfg.horizontal_scrollbar, content_size.0, viewport_size.0);
        }
    }

    let scrollbars = (scrollable && host_scrollbars).then(|| {
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
        Scrollbars {
            viewport: viewport_local,
            content: content_local,
            vbar,
            hbar,
            thickness,
        }
    });

    ResolvedScrollView {
        inner,
        viewport_size,
        content_size,
        scrollbars,
        show_v,
        show_h,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_scrollbars(
    frame: &mut Frame<'_>,
    area: Rect,
    scrollbars: Scrollbars,
    viewport_size: (u16, u16),
    content_size: (u16, u16),
    scroll: ScrollOffset,
    cfg: ScrollConfig,
    theme: &Theme,
) {
    let track_style = theme.scrollbar_track;
    let thumb_style = theme.scrollbar_thumb;
    let arrow_style = theme.scrollbar_arrow;
    let buf = frame.buffer_mut();

    let track = theme.glyph("scrollbar-track").unwrap_or("\u{2591}");
    let thumb = theme.glyph("scrollbar-thumb").unwrap_or("\u{2588}");
    let arrow_up = theme.glyph("scrollbar-up-arrow").unwrap_or("\u{25B2}");
    let arrow_down = theme.glyph("scrollbar-down-arrow").unwrap_or("\u{25BC}");
    let arrow_left = theme.glyph("scrollbar-left-arrow").unwrap_or("\u{25C4}");
    let arrow_right = theme.glyph("scrollbar-right-arrow").unwrap_or("\u{25BA}");

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

pub(crate) fn max_scroll_offset(content: (u16, u16), viewport: (u16, u16)) -> ScrollOffset {
    let max_x = content.0.saturating_sub(viewport.0);
    let max_y = content.1.saturating_sub(viewport.1);
    ScrollOffset { x: max_x, y: max_y }
}

pub(crate) fn clamp_scroll_offset(
    content: (u16, u16),
    viewport: (u16, u16),
    desired: ScrollOffset,
) -> ScrollOffset {
    let max = max_scroll_offset(content, viewport);
    ScrollOffset {
        x: desired.x.min(max.x),
        y: desired.y.min(max.y),
    }
}

pub fn should_show_scrollbar(
    vis: ScrollbarVisibility,
    content_len: u16,
    viewport_len: u16,
) -> bool {
    match vis {
        ScrollbarVisibility::Always => true,
        ScrollbarVisibility::Auto => content_len > viewport_len,
        ScrollbarVisibility::Never => false,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScrollbarThumb {
    /// Offset within the track (0-based).
    pub start: u16,
    /// Length in cells.
    pub len: u16,
}

pub(crate) fn scrollbar_thumb(
    track_len: u16,
    viewport_len: u16,
    content_len: u16,
    offset: u16,
) -> ScrollbarThumb {
    if track_len == 0 {
        return ScrollbarThumb::default();
    }
    if content_len == 0 {
        return ScrollbarThumb {
            start: 0,
            len: track_len,
        };
    }
    if content_len <= viewport_len {
        return ScrollbarThumb {
            start: 0,
            len: track_len,
        };
    }

    let track_len_u32 = track_len as u32;
    let viewport_len_u32 = cmp::max(1, viewport_len) as u32;
    let content_len_u32 = content_len as u32;

    let mut thumb_len = ((track_len_u32 * viewport_len_u32) / content_len_u32)
        .min(track_len_u32)
        .max(1) as u16;
    if thumb_len > track_len {
        thumb_len = track_len;
    }

    let max_offset = content_len.saturating_sub(viewport_len) as u32;
    let max_thumb_start = track_len.saturating_sub(thumb_len) as u32;
    if max_offset == 0 || max_thumb_start == 0 {
        return ScrollbarThumb {
            start: 0,
            len: thumb_len,
        };
    }

    let off = offset.min(content_len.saturating_sub(viewport_len)) as u32;
    let start = ((off * max_thumb_start) / max_offset).min(max_thumb_start) as u16;
    ScrollbarThumb {
        start,
        len: thumb_len,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollbarLayout1D {
    /// Total bar length (cells along the scrolling axis).
    pub bar_len: u16,
    /// Whether arrow buttons are present.
    pub has_arrows: bool,
    /// Track start offset within the bar.
    pub track_start: u16,
    /// Track length in cells (excludes arrow buttons).
    pub track_len: u16,
    /// Thumb start within the bar (absolute bar coordinates).
    pub thumb_start: u16,
    /// Thumb length in cells.
    pub thumb_len: u16,
}

pub fn scrollbar_layout_1d(
    bar_len: u16,
    viewport_len: u16,
    content_len: u16,
    offset: u16,
    arrows: bool,
) -> ScrollbarLayout1D {
    if bar_len == 0 {
        return ScrollbarLayout1D::default();
    }

    let has_arrows = arrows && bar_len >= 2;
    let track_start: u16 = if has_arrows { 1 } else { 0 };
    let track_len = if has_arrows {
        bar_len.saturating_sub(2)
    } else {
        bar_len
    };

    let thumb = scrollbar_thumb(track_len, viewport_len, content_len, offset);
    let thumb_start = track_start.saturating_add(thumb.start);

    ScrollbarLayout1D {
        bar_len,
        has_arrows,
        track_start,
        track_len,
        thumb_start,
        thumb_len: thumb.len,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarHit {
    ArrowDec,
    ArrowInc,
    TrackDec,
    TrackInc,
    Thumb { grab_offset: u16 },
    None,
}

pub fn scrollbar_hit_test(layout: ScrollbarLayout1D, pos: u16) -> ScrollbarHit {
    if layout.bar_len == 0 || pos >= layout.bar_len {
        return ScrollbarHit::None;
    }

    if layout.has_arrows {
        if pos == 0 {
            return ScrollbarHit::ArrowDec;
        }
        if pos == layout.bar_len.saturating_sub(1) {
            return ScrollbarHit::ArrowInc;
        }
    }

    if layout.track_len == 0 {
        return ScrollbarHit::None;
    }

    if pos < layout.track_start || pos >= layout.track_start.saturating_add(layout.track_len) {
        return ScrollbarHit::None;
    }

    let rel = pos.saturating_sub(layout.track_start);
    let thumb_start_rel = layout.thumb_start.saturating_sub(layout.track_start);
    let thumb_end_rel = thumb_start_rel.saturating_add(layout.thumb_len);

    if rel >= thumb_start_rel && rel < thumb_end_rel {
        return ScrollbarHit::Thumb {
            grab_offset: rel.saturating_sub(thumb_start_rel),
        };
    }

    if rel < thumb_start_rel {
        ScrollbarHit::TrackDec
    } else {
        ScrollbarHit::TrackInc
    }
}

pub fn scroll_offset_from_thumb_start(
    track_len: u16,
    viewport_len: u16,
    content_len: u16,
    thumb_start: u16,
) -> u16 {
    if track_len == 0 {
        return 0;
    }
    if content_len <= viewport_len {
        return 0;
    }

    let thumb = scrollbar_thumb(track_len, viewport_len, content_len, 0);
    let max_offset = content_len.saturating_sub(viewport_len) as u32;
    let max_thumb_start = track_len.saturating_sub(thumb.len) as u32;
    if max_offset == 0 || max_thumb_start == 0 {
        return 0;
    }

    let start = (thumb_start.min(track_len.saturating_sub(thumb.len))) as u32;
    ((start * max_offset) / max_thumb_start).min(max_offset) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    };

    #[test]
    fn scroll_offset_clamps_to_content_minus_viewport() {
        let content = (100, 50);
        let viewport = (20, 10);
        let max = max_scroll_offset(content, viewport);
        assert_eq!(max, ScrollOffset { x: 80, y: 40 });

        let desired = ScrollOffset { x: 200, y: 200 };
        let clamped = clamp_scroll_offset(content, viewport, desired);
        assert_eq!(clamped, max);

        let desired = ScrollOffset { x: 10, y: 5 };
        let clamped = clamp_scroll_offset(content, viewport, desired);
        assert_eq!(clamped, desired);
    }

    #[test]
    fn scroll_offset_is_zero_when_content_fits() {
        let content = (10, 5);
        let viewport = (20, 10);
        let max = max_scroll_offset(content, viewport);
        assert_eq!(max, ScrollOffset::ZERO);

        let clamped = clamp_scroll_offset(content, viewport, ScrollOffset { x: 5, y: 5 });
        assert_eq!(clamped, ScrollOffset::ZERO);
    }

    #[test]
    fn scroll_input_keys_map_to_shared_delta_logic() {
        let cfg = ScrollConfig::default();
        let content = (100, 80);
        let viewport = (10, 5);
        let scroll = ScrollOffset { x: 5, y: 10 };

        let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));

        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &key(KeyCode::Up)),
            Some(ScrollOffset { x: 5, y: 9 })
        );
        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &key(KeyCode::Down)),
            Some(ScrollOffset { x: 5, y: 11 })
        );
        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &key(KeyCode::Left)),
            Some(ScrollOffset { x: 4, y: 10 })
        );
        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &key(KeyCode::Right)),
            Some(ScrollOffset { x: 6, y: 10 })
        );
        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &key(KeyCode::PageUp)),
            Some(ScrollOffset { x: 5, y: 5 })
        );
        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &key(KeyCode::PageDown)),
            Some(ScrollOffset { x: 5, y: 15 })
        );
        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &key(KeyCode::Home)),
            Some(ScrollOffset::ZERO)
        );
        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &key(KeyCode::End)),
            Some(ScrollOffset { x: 90, y: 75 })
        );
    }

    #[test]
    fn scroll_input_ignores_key_release_and_unhandled_events() {
        let cfg = ScrollConfig::default();
        let content = (100, 80);
        let viewport = (10, 5);
        let scroll = ScrollOffset { x: 5, y: 10 };
        let release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        let unhandled = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &release),
            None
        );
        assert_eq!(
            scroll_offset_for_input_event(cfg, content, viewport, scroll, &unhandled),
            None
        );
    }

    #[test]
    fn scroll_input_mouse_wheel_uses_configured_step() {
        let cfg = ScrollConfig::default().wheel_step(4);
        let content = (100, 80);
        let viewport = (10, 5);
        let scroll = ScrollOffset { x: 5, y: 10 };
        let mouse = |kind| {
            Event::Mouse(MouseEvent {
                kind,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            })
        };

        assert_eq!(
            scroll_offset_for_input_event(
                cfg,
                content,
                viewport,
                scroll,
                &mouse(MouseEventKind::ScrollUp),
            ),
            Some(ScrollOffset { x: 5, y: 6 })
        );
        assert_eq!(
            scroll_offset_for_input_event(
                cfg,
                content,
                viewport,
                scroll,
                &mouse(MouseEventKind::ScrollDown),
            ),
            Some(ScrollOffset { x: 5, y: 14 })
        );
        assert_eq!(
            scroll_offset_for_input_event(
                cfg,
                content,
                viewport,
                scroll,
                &mouse(MouseEventKind::ScrollLeft),
            ),
            Some(ScrollOffset { x: 1, y: 10 })
        );
        assert_eq!(
            scroll_offset_for_input_event(
                cfg,
                content,
                viewport,
                scroll,
                &mouse(MouseEventKind::ScrollRight),
            ),
            Some(ScrollOffset { x: 9, y: 10 })
        );
    }

    #[test]
    fn scrollbar_thumb_tracks_offset() {
        // Track is 10 cells, viewport 10 lines, content 100 lines -> thumb is 1 cell.
        let thumb_top = scrollbar_thumb(10, 10, 100, 0);
        assert_eq!(thumb_top.start, 0);
        assert_eq!(thumb_top.len, 1);

        let thumb_bottom = scrollbar_thumb(10, 10, 100, 90);
        assert_eq!(thumb_bottom.start, 9);
        assert_eq!(thumb_bottom.len, 1);

        // Content fits -> thumb spans full track.
        let thumb_full = scrollbar_thumb(10, 10, 10, 0);
        assert_eq!(thumb_full.start, 0);
        assert_eq!(thumb_full.len, 10);
    }

    #[test]
    fn scroll_offset_from_thumb_start_maps_to_range() {
        // Track 10, viewport 10, content 100 => max offset 90 and max thumb start 9.
        assert_eq!(scroll_offset_from_thumb_start(10, 10, 100, 0), 0);
        assert_eq!(scroll_offset_from_thumb_start(10, 10, 100, 9), 90);

        // Midpoint is approximate due to integer division, but should be monotonic.
        let mid = scroll_offset_from_thumb_start(10, 10, 100, 4);
        assert!(mid > 0 && mid < 90);
    }

    #[test]
    fn scrollbar_visibility_modes() {
        assert!(should_show_scrollbar(ScrollbarVisibility::Always, 10, 10));
        assert!(!should_show_scrollbar(ScrollbarVisibility::Never, 100, 10));
        assert!(!should_show_scrollbar(ScrollbarVisibility::Auto, 10, 10));
        assert!(should_show_scrollbar(ScrollbarVisibility::Auto, 11, 10));
    }

    #[test]
    fn scrollbar_layout_reserves_arrow_buttons() {
        let layout = scrollbar_layout_1d(10, 10, 100, 0, true);
        assert!(layout.has_arrows);
        assert_eq!(layout.track_start, 1);
        assert_eq!(layout.track_len, 8);

        let layout_no_arrows = scrollbar_layout_1d(10, 10, 100, 0, false);
        assert!(!layout_no_arrows.has_arrows);
        assert_eq!(layout_no_arrows.track_start, 0);
        assert_eq!(layout_no_arrows.track_len, 10);
    }

    #[test]
    fn scrollbar_hit_test_detects_arrows_and_thumb() {
        // Track 8, content 100, viewport 10 => thumb len 1.
        let layout = scrollbar_layout_1d(10, 10, 100, 0, true);
        assert_eq!(scrollbar_hit_test(layout, 0), ScrollbarHit::ArrowDec);
        assert_eq!(scrollbar_hit_test(layout, 9), ScrollbarHit::ArrowInc);

        // At top, thumb starts at track_start (pos=1).
        assert_eq!(
            scrollbar_hit_test(layout, 1),
            ScrollbarHit::Thumb { grab_offset: 0 }
        );
        assert_eq!(scrollbar_hit_test(layout, 2), ScrollbarHit::TrackInc);
    }
}
