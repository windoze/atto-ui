// Rendering helpers shared by the unified view and the side-by-side panes.

use editor_core::Cell;
use editor_core_diff::DiffLineKind;
use editor_core_diff_view::{DiffProjection, Gutter, RowSlot};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::theme::EditorTheme;

/// How the gutter for a rendered column should be laid out.
#[derive(Clone, Copy, Debug)]
pub(crate) enum GutterLayout {
    /// Unified single column: show both before and after line numbers.
    Unified {
        before_digits: usize,
        after_digits: usize,
    },
    /// One side of a side-by-side view: show that side's line number only.
    Side { side: usize, digits: usize },
}

pub(crate) fn line_number_digits(line_count: usize) -> usize {
    line_count.to_string().len().max(2)
}

pub(crate) fn gutter_width(layout: GutterLayout, show_line_numbers: bool) -> u16 {
    // Trailing "marker + space" is always present so `+`/`-` have a stable column.
    let marker = 2u16;
    let numbers = if show_line_numbers {
        match layout {
            GutterLayout::Unified {
                before_digits,
                after_digits,
            } => (before_digits + 1 + after_digits + 1) as u16,
            GutterLayout::Side { digits, .. } => (digits + 1) as u16,
        }
    } else {
        0
    };
    numbers + marker
}

fn marker_style(theme: &EditorTheme, change: DiffLineKind) -> Style {
    match change {
        DiffLineKind::Add => theme.gutter.fg(Color::Green),
        DiffLineKind::Remove => theme.gutter.fg(Color::Red),
        DiffLineKind::Context => theme.gutter,
    }
}

/// Resolves the diff-cell style ids (add/remove/spacer backgrounds) onto the base text style.
fn style_for_cell(theme: &EditorTheme, cell: &Cell) -> Style {
    let mut fg = None;
    let mut bg = None;
    let mut mods = Modifier::empty();
    for id in &cell.styles {
        if let Some(style) = theme.style_ids.get(id) {
            if style.fg.is_some() {
                fg = style.fg;
            }
            if style.bg.is_some() {
                bg = style.bg;
            }
            mods |= style.add_modifier;
        }
    }
    let mut style = theme.text;
    if let Some(fg) = fg {
        style = style.fg(fg);
    }
    if let Some(bg) = bg {
        style = style.bg(bg);
    }
    style.add_modifier(mods)
}

fn gutter_spans(
    theme: &EditorTheme,
    layout: GutterLayout,
    show_line_numbers: bool,
    gutter: Gutter,
    change: DiffLineKind,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    if show_line_numbers {
        match layout {
            GutterLayout::Unified {
                before_digits,
                after_digits,
            } => {
                spans.push(Span::styled(
                    number_cell(gutter.before_line, before_digits),
                    theme.gutter,
                ));
                spans.push(Span::styled(" ", theme.gutter));
                spans.push(Span::styled(
                    number_cell(gutter.after_line, after_digits),
                    theme.gutter,
                ));
                spans.push(Span::styled(" ", theme.gutter));
            }
            GutterLayout::Side { side, digits } => {
                let value = match side {
                    0 => gutter.before_line,
                    _ => gutter.after_line,
                };
                spans.push(Span::styled(number_cell(value, digits), theme.gutter));
                spans.push(Span::styled(" ", theme.gutter));
            }
        }
    }

    let marker = gutter.marker.unwrap_or(' ');
    spans.push(Span::styled(
        marker.to_string(),
        marker_style(theme, change),
    ));
    spans.push(Span::styled(" ", theme.gutter));
    spans
}

fn number_cell(value: Option<usize>, digits: usize) -> String {
    match value {
        Some(n) => format!("{:>width$}", n + 1, width = digits),
        None => " ".repeat(digits),
    }
}

fn content_spans(theme: &EditorTheme, cells: &[Cell]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buffer = String::new();
    let mut current: Option<Style> = None;

    for cell in cells {
        let style = style_for_cell(theme, cell);
        match current {
            Some(s) if s == style => {}
            Some(s) => {
                spans.push(Span::styled(std::mem::take(&mut buffer), s));
                current = Some(style);
            }
            None => current = Some(style),
        }
        buffer.push(cell.ch);
    }
    if !buffer.is_empty() {
        spans.push(Span::styled(buffer, current.unwrap_or(theme.text)));
    }
    spans
}

/// Renders one projected column (`column`) into `area`, starting at unified row `scroll_top`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_column(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: &EditorTheme,
    projection: &DiffProjection,
    column: usize,
    scroll_top: usize,
    layout: GutterLayout,
    show_line_numbers: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Block::default().style(theme.background), area);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(area.height as usize);
    for i in 0..(area.height as usize) {
        let row_idx = scroll_top + i;
        let Some(row) = projection.rows().get(row_idx) else {
            lines.push(Line::from(""));
            continue;
        };
        let Some(slot) = row.slots().get(column) else {
            lines.push(Line::from(""));
            continue;
        };

        let (cells, gutter, change) = match slot {
            RowSlot::Line {
                cells,
                gutter,
                change,
                ..
            } => (cells.as_slice(), *gutter, *change),
            RowSlot::Spacer { cells, change, .. } => (cells.as_slice(), Gutter::empty(), *change),
        };

        let mut spans = gutter_spans(theme, layout, show_line_numbers, gutter, change);
        spans.extend(content_spans(theme, cells));
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines).style(theme.text), area);
}
