use std::cmp;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;

use atto_ui::composable::{ScrollContentContext, ScrollOffset, scrollbar_layout_1d};
use atto_ui::theme::Theme;

use super::MarkdownShared;
use super::embedded_scrollbar::{CodeBlockState, EmbeddedScrollView, TableBlockState};
use super::layout::{
    LayoutBlock, LayoutBlockKind, LineLayout, PrefixSpec, TextBlockStyle, TextKind,
};
use super::parser::{InlineSpan, SpanKind};
use super::styles::MarkdownStyles;
use super::text::{slice_by_width, text_width};

pub(super) fn draw_content(
    shared: &mut MarkdownShared,
    frame: &mut Frame<'_>,
    area: Rect,
    ctx: ScrollContentContext<'_>,
) {
    let layout = shared.cache.layout.clone();
    let Some(layout) = layout else {
        return;
    };

    let styles = MarkdownStyles::resolve(ctx.component.theme, shared);
    frame.render_widget(Block::default().style(styles.base), area);

    let scroll = ctx.info.scroll_offset;
    let viewport_h = area.height;
    let viewport_w = area.width;
    let content_width = layout.wrap_width.min(viewport_w);
    if viewport_h == 0 || viewport_w == 0 {
        return;
    }

    for block in layout.blocks.iter() {
        if block.y >= scroll.y.saturating_add(viewport_h) {
            break;
        }
        if block.y.saturating_add(block.height) <= scroll.y {
            continue;
        }

        match &block.kind {
            LayoutBlockKind::Text { lines, style } => {
                let block_start = block.y;
                let mut line_idx: u16 = 0;
                for line in lines.iter() {
                    let content_y = block_start.saturating_add(line_idx);
                    line_idx = line_idx.saturating_add(1);
                    if content_y < scroll.y {
                        continue;
                    }
                    if content_y >= scroll.y.saturating_add(viewport_h) {
                        break;
                    }
                    let y = area.y.saturating_add(content_y.saturating_sub(scroll.y));
                    draw_line(frame, area.x, y, content_width, line, style, &styles);
                }
            }
            LayoutBlockKind::Code {
                index,
                prefix,
                in_blockquote,
            } => {
                if let Some(code) = shared.cache.code_blocks.get_mut(*index) {
                    draw_code_block(
                        frame,
                        area,
                        block,
                        code,
                        prefix,
                        scroll,
                        content_width,
                        &styles,
                        ctx.component.theme,
                        *in_blockquote,
                    );
                }
            }
            LayoutBlockKind::Table {
                index,
                prefix,
                in_blockquote,
            } => {
                if let Some(table) = shared.cache.tables.get_mut(*index) {
                    draw_table_block(
                        frame,
                        area,
                        block,
                        table,
                        prefix,
                        scroll,
                        content_width,
                        &styles,
                        ctx.component.theme,
                        *in_blockquote,
                    );
                }
            }
        }
    }
}

fn draw_line(
    frame: &mut Frame<'_>,
    x: u16,
    y: u16,
    width: u16,
    line: &LineLayout,
    block_style: &TextBlockStyle,
    styles: &MarkdownStyles,
) {
    if width == 0 {
        return;
    }
    let base = base_style_for_block(block_style, styles);
    let spans = styled_spans(&line.spans, base, styles);
    draw_spans_with_scroll(frame, x, y, width, &spans, 0);
}

#[allow(clippy::too_many_arguments)]
fn draw_code_block(
    frame: &mut Frame<'_>,
    area: Rect,
    block: &LayoutBlock,
    code: &mut CodeBlockState,
    prefix: &PrefixSpec,
    scroll: ScrollOffset,
    wrap_width: u16,
    styles: &MarkdownStyles,
    theme: &Theme,
    in_blockquote: bool,
) {
    let prefix_width = prefix.first_width.max(prefix.rest_width);
    let total_width = wrap_width.min(area.width);
    let content_x = area.x.saturating_add(prefix_width);
    let content_width = total_width.saturating_sub(prefix_width);
    if content_width == 0 {
        return;
    }

    let block_start = block.y;
    let block_end = block.y.saturating_add(block.height);
    let visible_start = block_start.max(scroll.y);
    let visible_end = block_end.min(scroll.y.saturating_add(area.height));
    if visible_start >= visible_end {
        return;
    }

    let prefix_style = if in_blockquote {
        styles.blockquote
    } else {
        styles.base
    };
    let code_style = styles.code_block;

    let (content_w, content_h) = code.content_size();
    let embedded =
        EmbeddedScrollView::solve_auto((content_w, content_h), (content_width, block.height));
    let viewport_w = embedded.viewport_w;
    let viewport_h = embedded.viewport_h;

    let max_x = content_w.saturating_sub(viewport_w);
    let max_y = content_h.saturating_sub(viewport_h);
    let content_scroll = ScrollOffset {
        x: code.scroll.x.min(max_x),
        y: code.scroll.y.min(max_y),
    };
    if content_scroll != code.scroll {
        code.scroll = content_scroll;
    }

    let arrows = true;
    let v_layout = embedded.show_v.then_some(scrollbar_layout_1d(
        viewport_h,
        viewport_h,
        content_h,
        content_scroll.y,
        arrows,
    ));
    let h_layout = embedded.show_h.then_some(scrollbar_layout_1d(
        viewport_w,
        viewport_w,
        content_w,
        content_scroll.x,
        arrows,
    ));

    let track_style = theme.scrollbar_track;
    let thumb_style = theme.scrollbar_thumb;
    let arrow_style = theme.scrollbar_arrow;
    let track = theme.glyph("scrollbar-track").unwrap_or("░");
    let thumb = theme.glyph("scrollbar-thumb").unwrap_or("█");
    let arrow_up = theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
    let arrow_down = theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
    let arrow_left = theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
    let arrow_right = theme.glyph("scrollbar-right-arrow").unwrap_or("►");

    for line_offset in visible_start..visible_end {
        let local_line = line_offset.saturating_sub(block_start);
        let screen_y = area.y.saturating_add(line_offset.saturating_sub(scroll.y));
        let prefix_spans = if local_line == 0 {
            &prefix.first
        } else {
            &prefix.rest
        };
        let styled_prefix = styled_prefix_spans(prefix_spans, prefix_style, styles);
        draw_spans_with_scroll(frame, area.x, screen_y, prefix_width, &styled_prefix, 0);

        if embedded.show_h && local_line >= viewport_h {
            let Some(layout) = h_layout else {
                continue;
            };

            let buf = frame.buffer_mut();
            for dx in 0..viewport_w {
                let (symbol, bar_style) = if layout.has_arrows && dx == 0 {
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
                if let Some(cell) = buf.cell_mut((content_x.saturating_add(dx), screen_y)) {
                    cell.set_symbol(symbol);
                    cell.set_style(code_style.patch(bar_style));
                }
            }

            if embedded.show_v {
                if let Some(cell) = buf.cell_mut((content_x.saturating_add(viewport_w), screen_y)) {
                    cell.set_symbol(track);
                    cell.set_style(code_style.patch(track_style));
                }
            }
            continue;
        }

        let code_line_idx = content_scroll.y.saturating_add(local_line);
        let line = code
            .lines
            .get(code_line_idx as usize)
            .cloned()
            .unwrap_or_default();
        fill_line(frame, content_x, screen_y, viewport_w, code_style);
        let (segment, _) = slice_by_width(&line, content_scroll.x, viewport_w);
        let styled = vec![StyledSpan {
            text: segment,
            style: code_style,
        }];
        draw_spans_with_scroll(frame, content_x, screen_y, viewport_w, &styled, 0);

        if embedded.show_v {
            let Some(layout) = v_layout else {
                continue;
            };

            let dy = local_line.min(layout.bar_len.saturating_sub(1));
            let (symbol, bar_style) = if layout.has_arrows && dy == 0 {
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

            let buf = frame.buffer_mut();
            if let Some(cell) = buf.cell_mut((content_x.saturating_add(viewport_w), screen_y)) {
                cell.set_symbol(symbol);
                cell.set_style(code_style.patch(bar_style));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_table_block(
    frame: &mut Frame<'_>,
    area: Rect,
    block: &LayoutBlock,
    table: &mut TableBlockState,
    prefix: &PrefixSpec,
    scroll: ScrollOffset,
    wrap_width: u16,
    styles: &MarkdownStyles,
    theme: &Theme,
    in_blockquote: bool,
) {
    let prefix_width = prefix.first_width.max(prefix.rest_width);
    let total_width = wrap_width.min(area.width);
    let content_x = area.x.saturating_add(prefix_width);
    let content_width = total_width.saturating_sub(prefix_width);
    if content_width == 0 {
        return;
    }

    let (table_width, table_height) = table.content_size();
    if table_width == 0 || table_height == 0 {
        return;
    }

    let block_start = block.y;
    let block_end = block.y.saturating_add(block.height);
    let visible_start = block_start.max(scroll.y);
    let visible_end = block_end.min(scroll.y.saturating_add(area.height));
    if visible_start >= visible_end {
        return;
    }

    let prefix_style = if in_blockquote {
        styles.blockquote
    } else {
        styles.base
    };

    let (content_w, content_h) = table.content_size();
    let embedded =
        EmbeddedScrollView::solve_auto((content_w, content_h), (content_width, block.height));
    let viewport_w = embedded.viewport_w;
    let viewport_h = embedded.viewport_h;

    let max_x = content_w.saturating_sub(viewport_w);
    let max_y = content_h.saturating_sub(viewport_h);
    let content_scroll = ScrollOffset {
        x: table.scroll.x.min(max_x),
        y: table.scroll.y.min(max_y),
    };
    if content_scroll != table.scroll {
        table.scroll = content_scroll;
    }

    let arrows = true;
    let v_layout = embedded.show_v.then_some(scrollbar_layout_1d(
        viewport_h,
        viewport_h,
        content_h,
        content_scroll.y,
        arrows,
    ));
    let h_layout = embedded.show_h.then_some(scrollbar_layout_1d(
        viewport_w,
        viewport_w,
        content_w,
        content_scroll.x,
        arrows,
    ));

    let track_style = theme.scrollbar_track;
    let thumb_style = theme.scrollbar_thumb;
    let arrow_style = theme.scrollbar_arrow;
    let track = theme.glyph("scrollbar-track").unwrap_or("░");
    let thumb = theme.glyph("scrollbar-thumb").unwrap_or("█");
    let arrow_up = theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
    let arrow_down = theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
    let arrow_left = theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
    let arrow_right = theme.glyph("scrollbar-right-arrow").unwrap_or("►");

    for line_offset in visible_start..visible_end {
        let local_line = line_offset.saturating_sub(block_start);
        let screen_y = area.y.saturating_add(line_offset.saturating_sub(scroll.y));

        let prefix_spans = if local_line == 0 {
            &prefix.first
        } else {
            &prefix.rest
        };
        let styled_prefix = styled_prefix_spans(prefix_spans, prefix_style, styles);
        draw_spans_with_scroll(frame, area.x, screen_y, prefix_width, &styled_prefix, 0);

        if embedded.show_h && local_line >= viewport_h {
            let Some(layout) = h_layout else {
                continue;
            };

            let buf = frame.buffer_mut();
            for dx in 0..viewport_w {
                let (symbol, bar_style) = if layout.has_arrows && dx == 0 {
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
                if let Some(cell) = buf.cell_mut((content_x.saturating_add(dx), screen_y)) {
                    cell.set_symbol(symbol);
                    cell.set_style(styles.table_cell.patch(bar_style));
                }
            }

            if embedded.show_v {
                if let Some(cell) = buf.cell_mut((content_x.saturating_add(viewport_w), screen_y)) {
                    cell.set_symbol(track);
                    cell.set_style(styles.table_cell.patch(track_style));
                }
            }
            continue;
        }

        let table_line = content_scroll.y.saturating_add(local_line);
        let spans = table_line_spans(table, table_line, styles);
        draw_spans_with_scroll(
            frame,
            content_x,
            screen_y,
            viewport_w,
            &spans,
            content_scroll.x,
        );

        if embedded.show_v {
            let Some(layout) = v_layout else {
                continue;
            };

            let dy = local_line.min(layout.bar_len.saturating_sub(1));
            let (symbol, bar_style) = if layout.has_arrows && dy == 0 {
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

            let buf = frame.buffer_mut();
            if let Some(cell) = buf.cell_mut((content_x.saturating_add(viewport_w), screen_y)) {
                cell.set_symbol(symbol);
                cell.set_style(styles.table_cell.patch(bar_style));
            }
        }
    }
}

fn table_line_spans(
    table: &TableBlockState,
    line: u16,
    styles: &MarkdownStyles,
) -> Vec<StyledSpan> {
    let (width, height) = table.content_size();
    if line >= height || width == 0 {
        return Vec::new();
    }

    let mut line_idx = 0u16;

    if line == line_idx {
        return border_line_spans(table, TableBorderLineKind::Top, styles);
    }
    line_idx = line_idx.saturating_add(1);

    if !table.headers.is_empty() {
        if line == line_idx {
            return row_line_spans(table, &table.headers, true, styles);
        }
        line_idx = line_idx.saturating_add(1);
        if line == line_idx {
            return border_line_spans(table, TableBorderLineKind::Middle, styles);
        }
        line_idx = line_idx.saturating_add(1);
    }

    let body_index = line.saturating_sub(line_idx);
    if body_index < table.rows.len() as u16 {
        return row_line_spans(table, &table.rows[body_index as usize], false, styles);
    }

    border_line_spans(table, TableBorderLineKind::Bottom, styles)
}

#[derive(Clone, Copy, Debug)]
enum TableBorderLineKind {
    Top,
    Middle,
    Bottom,
}

fn border_line_spans(
    table: &TableBlockState,
    kind: TableBorderLineKind,
    styles: &MarkdownStyles,
) -> Vec<StyledSpan> {
    if table.col_widths.is_empty() {
        return Vec::new();
    }
    let glyphs = &styles.table_border_glyphs;
    let (left, join, right) = match kind {
        TableBorderLineKind::Top => (&glyphs.top_left, &glyphs.top_join, &glyphs.top_right),
        TableBorderLineKind::Middle => (&glyphs.left_join, &glyphs.center_join, &glyphs.right_join),
        TableBorderLineKind::Bottom => (
            &glyphs.bottom_left,
            &glyphs.bottom_join,
            &glyphs.bottom_right,
        ),
    };
    let mut text = String::new();
    text.push_str(left);
    for (idx, width) in table.col_widths.iter().enumerate() {
        let cell_w = width.saturating_add(2);
        text.push_str(&glyphs.horizontal.repeat(cell_w as usize));
        if idx + 1 < table.col_widths.len() {
            text.push_str(join);
        } else {
            text.push_str(right);
        }
    }
    vec![StyledSpan {
        text,
        style: styles.table_border,
    }]
}

fn row_line_spans(
    table: &TableBlockState,
    row: &[Vec<InlineSpan>],
    is_header: bool,
    styles: &MarkdownStyles,
) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    let border_style = styles.table_border;
    let vbar = styles.table_border_glyphs.vertical.clone();
    let cell_style = if is_header {
        styles.table_header
    } else {
        styles.table_cell
    };

    spans.push(StyledSpan {
        text: vbar.clone(),
        style: border_style,
    });

    for (col_idx, width) in table.col_widths.iter().enumerate() {
        spans.push(StyledSpan {
            text: " ".to_string(),
            style: cell_style,
        });
        let cell = row.get(col_idx).cloned().unwrap_or_default();
        let base_spans = styled_spans(&cell, cell_style, styles);
        spans.extend(base_spans);
        let cell_width = super::parser::spans_width(&cell);
        let pad = width.saturating_sub(cell_width);
        if pad > 0 {
            spans.push(StyledSpan {
                text: " ".repeat(pad as usize),
                style: cell_style,
            });
        }
        spans.push(StyledSpan {
            text: " ".to_string(),
            style: cell_style,
        });
        spans.push(StyledSpan {
            text: vbar.clone(),
            style: border_style,
        });
    }

    spans
}

fn styled_prefix_spans(
    spans: &[InlineSpan],
    base: Style,
    styles: &MarkdownStyles,
) -> Vec<StyledSpan> {
    spans
        .iter()
        .map(|span| {
            let style = match span.kind {
                SpanKind::Bullet => base.patch(styles.list_bullet),
                SpanKind::Marker => base.patch(styles.marker),
                SpanKind::Text => base,
            };
            StyledSpan {
                text: span.text.clone(),
                style,
            }
        })
        .collect()
}

fn styled_spans(spans: &[InlineSpan], base: Style, styles: &MarkdownStyles) -> Vec<StyledSpan> {
    let mut out = Vec::new();
    for span in spans {
        let mut style = base;
        match span.kind {
            SpanKind::Marker => {
                style = style.patch(styles.marker);
            }
            SpanKind::Bullet => {
                style = style.patch(styles.list_bullet);
            }
            SpanKind::Text => {
                if span.inline.code {
                    style = style.patch(styles.code_inline);
                }
                if span.inline.bold {
                    style = style.patch(styles.bold);
                }
                if span.inline.italic {
                    style = style.patch(styles.italic);
                }
                if span.inline.strike {
                    style = style.patch(styles.strike);
                }
                if span.link.is_some() {
                    style = style.patch(styles.link);
                }
            }
        }
        out.push(StyledSpan {
            text: span.text.clone(),
            style,
        });
    }
    out
}

fn base_style_for_block(style: &TextBlockStyle, styles: &MarkdownStyles) -> Style {
    let base = if style.in_blockquote {
        styles.blockquote
    } else {
        styles.base
    };

    match style.kind {
        TextKind::Paragraph => base,
        TextKind::Heading(level) => {
            let idx = cmp::min((level.saturating_sub(1)) as usize, 5);
            base.patch(styles.heading[idx])
        }
    }
}

#[derive(Clone)]
struct StyledSpan {
    text: String,
    style: Style,
}

fn draw_spans_with_scroll(
    frame: &mut Frame<'_>,
    x: u16,
    y: u16,
    width: u16,
    spans: &[StyledSpan],
    scroll_x: u16,
) {
    if width == 0 {
        return;
    }
    let mut drawn: u16 = 0;
    let mut offset: u16 = 0;
    for span in spans {
        if drawn >= width {
            break;
        }
        let span_width = text_width(&span.text);
        if offset.saturating_add(span_width) <= scroll_x {
            offset = offset.saturating_add(span_width);
            continue;
        }
        let start = scroll_x.saturating_sub(offset);
        let available = width.saturating_sub(drawn);
        let (segment, segment_width) = slice_by_width(&span.text, start, available);
        if segment_width == 0 {
            offset = offset.saturating_add(span_width);
            continue;
        }
        frame.buffer_mut().set_stringn(
            x.saturating_add(drawn),
            y,
            segment,
            segment_width as usize,
            span.style,
        );
        drawn = drawn.saturating_add(segment_width);
        offset = offset.saturating_add(span_width);
    }
}

fn fill_line(frame: &mut Frame<'_>, x: u16, y: u16, width: u16, style: Style) {
    let buf = frame.buffer_mut();
    for dx in 0..width {
        if let Some(cell) = buf.cell_mut((x.saturating_add(dx), y)) {
            cell.set_symbol(" ");
            cell.set_style(style);
        }
    }
}
