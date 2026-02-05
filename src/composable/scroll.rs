use std::cmp;

use ratatui::layout::Rect;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Scrollbars {
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
pub(crate) enum ScrollbarDrag {
    Vertical { grab_offset: u16 },
    Horizontal { grab_offset: u16 },
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

pub(crate) fn should_show_scrollbar(
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
pub(crate) struct ScrollbarLayout1D {
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

pub(crate) fn scrollbar_layout_1d(
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
pub(crate) enum ScrollbarHit {
    ArrowDec,
    ArrowInc,
    TrackDec,
    TrackInc,
    Thumb { grab_offset: u16 },
    None,
}

pub(crate) fn scrollbar_hit_test(layout: ScrollbarLayout1D, pos: u16) -> ScrollbarHit {
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

pub(crate) fn scroll_offset_from_thumb_start(
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
