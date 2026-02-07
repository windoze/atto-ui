use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};

use atto_ui::composable::{
    ScrollOffset, ScrollbarDrag, ScrollbarHit, ScrollbarVisibility, scroll_offset_from_thumb_start,
    scrollbar_hit_test, scrollbar_layout_1d, should_show_scrollbar,
};

use super::layout::{LayoutBlock, LayoutBlockKind};
use super::parser::{InlineSpan, InlineStyle};
use super::text::{normalize_tabs, text_width};
use super::{EmbeddedScrollbarDragState, EmbeddedScrollbarTarget};

#[derive(Clone, Debug)]
pub(super) struct CodeBlockState {
    pub(super) lines: Vec<String>,
    pub(super) max_width: u16,
    pub(super) scroll: ScrollOffset,
}

impl CodeBlockState {
    pub(super) fn new(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(normalize_tabs).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let max_width = lines.iter().map(|s| text_width(s)).max().unwrap_or(0);
        Self {
            lines,
            max_width,
            scroll: ScrollOffset::ZERO,
        }
    }

    pub(super) fn content_size(&self) -> (u16, u16) {
        (
            self.max_width,
            self.lines.len().min(u16::MAX as usize) as u16,
        )
    }

    pub(super) fn handle_scroll(
        &mut self,
        m: MouseEvent,
        viewport_w: u16,
        viewport_h: u16,
        step: u16,
    ) -> bool {
        let (content_w, content_h) = self.content_size();
        let mut scroll = self.scroll;
        let mut changed = false;

        let kind = normalize_wheel_kind(m.kind, m.modifiers);
        match kind {
            MouseEventKind::ScrollUp => {
                let dy = step as i16;
                let new_y = scroll.y.saturating_sub(dy as u16);
                if new_y != scroll.y {
                    scroll.y = new_y;
                    changed = true;
                }
            }
            MouseEventKind::ScrollDown => {
                let max = content_h.saturating_sub(viewport_h);
                let new_y = scroll.y.saturating_add(step).min(max);
                if new_y != scroll.y {
                    scroll.y = new_y;
                    changed = true;
                }
            }
            MouseEventKind::ScrollLeft => {
                let dx = step as i16;
                let new_x = scroll.x.saturating_sub(dx as u16);
                if new_x != scroll.x {
                    scroll.x = new_x;
                    changed = true;
                }
            }
            MouseEventKind::ScrollRight => {
                let max = content_w.saturating_sub(viewport_w);
                let new_x = scroll.x.saturating_add(step).min(max);
                if new_x != scroll.x {
                    scroll.x = new_x;
                    changed = true;
                }
            }
            _ => {}
        }

        if changed {
            self.scroll = scroll;
        }
        changed
    }
}

#[derive(Clone, Debug)]
pub(super) struct TableBlockState {
    pub(super) headers: Vec<Vec<InlineSpan>>,
    pub(super) rows: Vec<Vec<Vec<InlineSpan>>>,
    pub(super) col_widths: Vec<u16>,
    pub(super) scroll: ScrollOffset,
}

impl TableBlockState {
    pub(super) fn new(headers: Vec<Vec<InlineSpan>>, rows: Vec<Vec<Vec<InlineSpan>>>) -> Self {
        let mut col_widths = Vec::new();
        let col_count = headers
            .len()
            .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
        col_widths.resize(col_count, 0);

        for (idx, cell) in headers.iter().enumerate() {
            col_widths[idx] = col_widths[idx].max(super::parser::spans_width(cell));
        }
        for row in rows.iter() {
            for (idx, cell) in row.iter().enumerate() {
                col_widths[idx] = col_widths[idx].max(super::parser::spans_width(cell));
            }
        }

        Self {
            headers,
            rows,
            col_widths,
            scroll: ScrollOffset::ZERO,
        }
    }

    pub(super) fn content_size(&self) -> (u16, u16) {
        let col_total: u16 = self.col_widths.iter().map(|w| w.saturating_add(2)).sum();
        let width = col_total.saturating_add(self.col_widths.len().saturating_add(1) as u16);
        let mut height: u16 = 0;
        if width == 0 {
            return (0, 0);
        }
        height = height.saturating_add(1); // top border
        if !self.headers.is_empty() {
            height = height.saturating_add(1); // header row
            height = height.saturating_add(1); // separator
        }
        height = height.saturating_add(self.rows.len().min(u16::MAX as usize) as u16);
        height = height.saturating_add(1); // bottom border
        (width, height)
    }

    pub(super) fn handle_scroll(
        &mut self,
        m: MouseEvent,
        viewport_w: u16,
        viewport_h: u16,
        step: u16,
    ) -> bool {
        let (content_w, content_h) = self.content_size();
        let mut scroll = self.scroll;
        let mut changed = false;

        let kind = normalize_wheel_kind(m.kind, m.modifiers);
        match kind {
            MouseEventKind::ScrollUp => {
                let dy = step as i16;
                let new_y = scroll.y.saturating_sub(dy as u16);
                if new_y != scroll.y {
                    scroll.y = new_y;
                    changed = true;
                }
            }
            MouseEventKind::ScrollDown => {
                let max = content_h.saturating_sub(viewport_h);
                let new_y = scroll.y.saturating_add(step).min(max);
                if new_y != scroll.y {
                    scroll.y = new_y;
                    changed = true;
                }
            }
            MouseEventKind::ScrollLeft => {
                let dx = step as i16;
                let new_x = scroll.x.saturating_sub(dx as u16);
                if new_x != scroll.x {
                    scroll.x = new_x;
                    changed = true;
                }
            }
            MouseEventKind::ScrollRight => {
                let max = content_w.saturating_sub(viewport_w);
                let new_x = scroll.x.saturating_add(step).min(max);
                if new_x != scroll.x {
                    scroll.x = new_x;
                    changed = true;
                }
            }
            _ => {}
        }

        if changed {
            self.scroll = scroll;
        }
        changed
    }

    pub(super) fn link_at(&self, col: u16, row: u16) -> Option<String> {
        let (_, height) = self.content_size();
        if row >= height {
            return None;
        }
        let line = self.scroll.y.saturating_add(row);
        let spans = table_line_raw_spans(self, line);
        let col = self.scroll.x.saturating_add(col);
        super::parser::link_at_in_spans(&spans, col)
    }
}

fn normalize_wheel_kind(kind: MouseEventKind, modifiers: KeyModifiers) -> MouseEventKind {
    if modifiers.contains(KeyModifiers::SHIFT) {
        match kind {
            MouseEventKind::ScrollUp => MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollDown => MouseEventKind::ScrollRight,
            _ => kind,
        }
    } else {
        kind
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct EmbeddedScrollView {
    pub(super) show_v: bool,
    pub(super) show_h: bool,
    pub(super) viewport_w: u16,
    pub(super) viewport_h: u16,
}

impl EmbeddedScrollView {
    pub(super) const THICKNESS: u16 = 1;

    pub(super) fn solve_auto(content: (u16, u16), outer: (u16, u16)) -> Self {
        let (content_w, content_h) = content;
        let (outer_w, outer_h) = outer;

        let mut show_v = false;
        let mut show_h = false;

        // Two-pass solve: scrollbar visibility affects viewport size, which can affect the other
        // scrollbar's visibility (e.g. vbar steals width, causing hbar).
        for _ in 0..2 {
            let viewport_w = outer_w.saturating_sub(if show_v { Self::THICKNESS } else { 0 });
            let viewport_h = outer_h.saturating_sub(if show_h { Self::THICKNESS } else { 0 });

            let can_show_v = outer_w > Self::THICKNESS && viewport_h > 0;
            let can_show_h = outer_h > Self::THICKNESS && viewport_w > 0;

            let new_show_v = can_show_v
                && should_show_scrollbar(ScrollbarVisibility::Auto, content_h, viewport_h);
            let new_show_h = can_show_h
                && should_show_scrollbar(ScrollbarVisibility::Auto, content_w, viewport_w);

            if new_show_v == show_v && new_show_h == show_h {
                break;
            }
            show_v = new_show_v;
            show_h = new_show_h;
        }

        let viewport_w = outer_w.saturating_sub(if show_v { Self::THICKNESS } else { 0 });
        let viewport_h = outer_h.saturating_sub(if show_h { Self::THICKNESS } else { 0 });

        Self {
            show_v,
            show_h,
            viewport_w,
            viewport_h,
        }
    }
}

pub(super) fn apply_embedded_scrollbar_drag(
    scroll: ScrollOffset,
    content: (u16, u16),
    embedded: EmbeddedScrollView,
    prefix_width: u16,
    local_x: u16,
    local_y: u16,
    drag: ScrollbarDrag,
) -> ScrollOffset {
    let mut scroll = scroll;

    match drag {
        ScrollbarDrag::Vertical { grab_offset } => {
            if !embedded.show_v || embedded.viewport_h == 0 {
                return scroll;
            }
            let bar_len = embedded.viewport_h;
            let layout =
                scrollbar_layout_1d(bar_len, embedded.viewport_h, content.1, scroll.y, true);
            if layout.track_len == 0 {
                return scroll;
            }

            let pos = local_y.min(bar_len.saturating_sub(1));
            let pos_in_track = pos
                .saturating_sub(layout.track_start)
                .min(layout.track_len.saturating_sub(1));
            let max_start = layout.track_len.saturating_sub(layout.thumb_len);
            let new_thumb_start = pos_in_track.saturating_sub(grab_offset).min(max_start);
            let new_y = scroll_offset_from_thumb_start(
                layout.track_len,
                embedded.viewport_h,
                content.1,
                new_thumb_start,
            );
            scroll.y = new_y;
        }
        ScrollbarDrag::Horizontal { grab_offset } => {
            if !embedded.show_h || embedded.viewport_w == 0 {
                return scroll;
            }
            let bar_len = embedded.viewport_w;
            let layout =
                scrollbar_layout_1d(bar_len, embedded.viewport_w, content.0, scroll.x, true);
            if layout.track_len == 0 {
                return scroll;
            }

            let local_x_in_bar = local_x.saturating_sub(prefix_width);
            let pos = local_x_in_bar.min(bar_len.saturating_sub(1));
            let pos_in_track = pos
                .saturating_sub(layout.track_start)
                .min(layout.track_len.saturating_sub(1));
            let max_start = layout.track_len.saturating_sub(layout.thumb_len);
            let new_thumb_start = pos_in_track.saturating_sub(grab_offset).min(max_start);
            let new_x = scroll_offset_from_thumb_start(
                layout.track_len,
                embedded.viewport_w,
                content.0,
                new_thumb_start,
            );
            scroll.x = new_x;
        }
    }

    let max_x = content.0.saturating_sub(embedded.viewport_w);
    let max_y = content.1.saturating_sub(embedded.viewport_h);
    scroll.x = scroll.x.min(max_x);
    scroll.y = scroll.y.min(max_y);

    scroll
}

pub(super) fn handle_embedded_scrollbar_mouse_down(
    drag_state: &mut Option<EmbeddedScrollbarDragState>,
    target: EmbeddedScrollbarTarget,
    scroll: ScrollOffset,
    content: (u16, u16),
    embedded: EmbeddedScrollView,
    local_x: u16,
    local_y: u16,
    prefix_width: u16,
) -> Option<ScrollOffset> {
    let mut scroll = scroll;

    let bar_x_v = prefix_width.saturating_add(embedded.viewport_w);
    let bar_y_h = embedded.viewport_h;
    let arrows = true;

    // Vertical scrollbar hit-test.
    if embedded.show_v
        && local_x == bar_x_v
        && embedded.viewport_h > 0
        && local_y < embedded.viewport_h
    {
        let layout = scrollbar_layout_1d(
            embedded.viewport_h,
            embedded.viewport_h,
            content.1,
            scroll.y,
            arrows,
        );
        let pos = local_y.min(layout.bar_len.saturating_sub(1));
        match scrollbar_hit_test(layout, pos) {
            ScrollbarHit::ArrowDec => scroll.y = scroll.y.saturating_sub(1),
            ScrollbarHit::ArrowInc => {
                let max = content.1.saturating_sub(embedded.viewport_h);
                scroll.y = scroll.y.saturating_add(1).min(max);
            }
            ScrollbarHit::TrackDec => {
                let page = embedded.viewport_h;
                scroll.y = scroll.y.saturating_sub(page);
            }
            ScrollbarHit::TrackInc => {
                let max = content.1.saturating_sub(embedded.viewport_h);
                let page = embedded.viewport_h;
                scroll.y = scroll.y.saturating_add(page).min(max);
            }
            ScrollbarHit::Thumb { grab_offset } => {
                *drag_state = Some(EmbeddedScrollbarDragState {
                    target,
                    drag: ScrollbarDrag::Vertical { grab_offset },
                });
            }
            ScrollbarHit::None => {}
        }
        return Some(scroll);
    }

    // Horizontal scrollbar hit-test (bottom row of the embedded viewport).
    if embedded.show_h
        && embedded.viewport_w > 0
        && local_y == bar_y_h
        && local_x >= prefix_width
        && local_x < prefix_width.saturating_add(embedded.viewport_w)
    {
        let layout = scrollbar_layout_1d(
            embedded.viewport_w,
            embedded.viewport_w,
            content.0,
            scroll.x,
            arrows,
        );
        let local_x_in_bar = local_x.saturating_sub(prefix_width);
        let pos = local_x_in_bar.min(layout.bar_len.saturating_sub(1));
        match scrollbar_hit_test(layout, pos) {
            ScrollbarHit::ArrowDec => scroll.x = scroll.x.saturating_sub(1),
            ScrollbarHit::ArrowInc => {
                let max = content.0.saturating_sub(embedded.viewport_w);
                scroll.x = scroll.x.saturating_add(1).min(max);
            }
            ScrollbarHit::TrackDec => {
                let page = embedded.viewport_w;
                scroll.x = scroll.x.saturating_sub(page);
            }
            ScrollbarHit::TrackInc => {
                let max = content.0.saturating_sub(embedded.viewport_w);
                let page = embedded.viewport_w;
                scroll.x = scroll.x.saturating_add(page).min(max);
            }
            ScrollbarHit::Thumb { grab_offset } => {
                *drag_state = Some(EmbeddedScrollbarDragState {
                    target,
                    drag: ScrollbarDrag::Horizontal { grab_offset },
                });
            }
            ScrollbarHit::None => {}
        }
        return Some(scroll);
    }

    None
}

// --- Raw table line construction (used for link hit-testing) -----------------

pub(super) fn table_line_raw_spans(table: &TableBlockState, line: u16) -> Vec<InlineSpan> {
    let (width, height) = table.content_size();
    if line >= height || width == 0 {
        return Vec::new();
    }

    let mut line_idx = 0u16;

    if line == line_idx {
        return border_line_raw_spans(table);
    }
    line_idx = line_idx.saturating_add(1);

    if !table.headers.is_empty() {
        if line == line_idx {
            return row_line_raw_spans(table, &table.headers);
        }
        line_idx = line_idx.saturating_add(1);
        if line == line_idx {
            return border_line_raw_spans(table);
        }
        line_idx = line_idx.saturating_add(1);
    }

    let body_index = line.saturating_sub(line_idx);
    if body_index < table.rows.len() as u16 {
        return row_line_raw_spans(table, &table.rows[body_index as usize]);
    }

    border_line_raw_spans(table)
}

fn border_line_raw_spans(table: &TableBlockState) -> Vec<InlineSpan> {
    if table.col_widths.is_empty() {
        return Vec::new();
    }
    let mut text = String::new();
    text.push('+');
    for width in &table.col_widths {
        let cell_w = width.saturating_add(2);
        text.push_str(&"-".repeat(cell_w as usize));
        text.push('+');
    }
    vec![InlineSpan::marker(&text)]
}

fn row_line_raw_spans(table: &TableBlockState, row: &[Vec<InlineSpan>]) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    spans.push(InlineSpan::marker("|"));
    for (col_idx, width) in table.col_widths.iter().enumerate() {
        spans.push(InlineSpan::text(" ", InlineStyle::default(), None));
        let cell = row.get(col_idx).cloned().unwrap_or_default();
        let cell_width = super::parser::spans_width(&cell);
        spans.extend(cell);
        let pad = width.saturating_sub(cell_width);
        if pad > 0 {
            spans.push(InlineSpan::text(
                &" ".repeat(pad as usize),
                InlineStyle::default(),
                None,
            ));
        }
        spans.push(InlineSpan::text(" ", InlineStyle::default(), None));
        spans.push(InlineSpan::marker("|"));
    }
    spans
}

// Helper for embedded scrollbars and hit-tests.
pub(super) fn prefix_width_for_block(block: &LayoutBlock) -> u16 {
    match &block.kind {
        LayoutBlockKind::Code { prefix, .. } | LayoutBlockKind::Table { prefix, .. } => {
            prefix.first_width.max(prefix.rest_width)
        }
        _ => 0,
    }
}
