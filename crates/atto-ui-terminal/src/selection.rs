use vt100::Screen;

/// Absolute cell position in the terminal's scrollback plus visible screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TerminalSelectionPosition {
    pub row: usize,
    pub col: u16,
}

impl TerminalSelectionPosition {
    pub const fn new(row: usize, col: u16) -> Self {
        Self { row, col }
    }
}

/// Normalized terminal selection range with an exclusive end position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSelectionRange {
    pub start: TerminalSelectionPosition,
    pub end: TerminalSelectionPosition,
}

impl TerminalSelectionRange {
    pub fn new(
        anchor: TerminalSelectionPosition,
        focus: TerminalSelectionPosition,
    ) -> Option<Self> {
        if anchor == focus {
            return None;
        }
        let (start, end) = if anchor <= focus {
            (anchor, focus)
        } else {
            (focus, anchor)
        };
        Some(Self { start, end })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TerminalSelectionState {
    anchor: Option<TerminalSelectionPosition>,
    focus: Option<TerminalSelectionPosition>,
    dragging: bool,
}

impl TerminalSelectionState {
    pub(crate) fn start(&mut self, position: TerminalSelectionPosition) {
        self.anchor = Some(position);
        self.focus = Some(position);
        self.dragging = true;
    }

    pub(crate) fn start_keyboard(&mut self, position: TerminalSelectionPosition) {
        self.anchor = Some(position);
        self.focus = Some(position);
        self.dragging = false;
    }

    pub(crate) fn update(&mut self, position: TerminalSelectionPosition) {
        if self.anchor.is_some() {
            self.focus = Some(position);
        }
    }

    pub(crate) fn finish(&mut self, position: TerminalSelectionPosition) {
        self.update(position);
        self.dragging = false;
    }

    pub(crate) fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub(crate) fn anchor(&self) -> Option<TerminalSelectionPosition> {
        self.anchor
    }

    pub(crate) fn clear(&mut self) -> bool {
        let had_selection = self.anchor.is_some() || self.focus.is_some();
        self.anchor = None;
        self.focus = None;
        self.dragging = false;
        had_selection
    }

    pub(crate) fn range(&self) -> Option<TerminalSelectionRange> {
        TerminalSelectionRange::new(self.anchor?, self.focus?)
    }
}

pub(crate) fn visible_top_row(max_scrollback: usize, scrollback_offset: usize) -> usize {
    max_scrollback.saturating_sub(scrollback_offset)
}

pub(crate) fn position_for_view_cell(
    max_scrollback: usize,
    scrollback_offset: usize,
    rows: u16,
    cols: u16,
    row: u16,
    col: u16,
) -> TerminalSelectionPosition {
    let row = row.min(rows.saturating_sub(1));
    let col = col.min(cols);
    TerminalSelectionPosition {
        row: visible_top_row(max_scrollback, scrollback_offset).saturating_add(usize::from(row)),
        col,
    }
}

pub(crate) fn selection_cols_for_row(
    range: TerminalSelectionRange,
    row: usize,
    row_width: u16,
) -> Option<(u16, u16)> {
    if row < range.start.row || row > range.end.row {
        return None;
    }

    let (start, end) = if range.start.row == range.end.row {
        (range.start.col.min(row_width), range.end.col.min(row_width))
    } else if row == range.start.row {
        (range.start.col.min(row_width), row_width)
    } else if row == range.end.row {
        (0, range.end.col.min(row_width))
    } else {
        (0, row_width)
    };
    (start < end).then_some((start, end))
}

pub(crate) fn selected_cell_ranges_for_screen_row(
    screen: &Screen,
    screen_row: u16,
    absolute_row: usize,
    row_width: u16,
    range: TerminalSelectionRange,
) -> Vec<(u16, u16)> {
    let Some((start_col, end_col)) = selection_cols_for_row(range, absolute_row, row_width) else {
        return Vec::new();
    };

    let mut ranges = Vec::new();
    let mut col = 0;
    while col < row_width {
        let cell = screen.cell(screen_row, col);
        if cell.is_some_and(vt100::Cell::is_wide_continuation) {
            col = col.saturating_add(1);
            continue;
        }

        let width = if cell.is_some_and(vt100::Cell::is_wide) {
            2
        } else {
            1
        };
        let next = col.saturating_add(width).min(row_width);
        if start_col < next && end_col > col {
            ranges.push((col, next));
        }
        col = next.max(col.saturating_add(1));
    }
    ranges
}

pub(crate) fn selected_text_from_screen(
    screen: &mut Screen,
    max_scrollback: usize,
    range: TerminalSelectionRange,
) -> Option<String> {
    let (rows, cols) = screen.size();
    if rows == 0 || cols == 0 {
        return None;
    }

    let total_rows = max_scrollback.saturating_add(usize::from(rows));
    let range = clamp_range(range, total_rows, cols)?;
    let original_scrollback = screen.scrollback();
    let mut out = String::new();

    for absolute_row in range.start.row..=range.end.row {
        let is_first = absolute_row == range.start.row;
        let is_last = absolute_row == range.end.row;
        let start_col = if is_first { range.start.col } else { 0 };
        let end_col = if is_last { range.end.col } else { cols };
        let local_row = make_absolute_row_visible(screen, max_scrollback, absolute_row);

        if !is_last && rows > 1 && local_row.saturating_add(1) < rows {
            out.push_str(&screen.contents_between(local_row, start_col, local_row + 1, 0));
        } else {
            out.push_str(&screen.contents_between(local_row, start_col, local_row, end_col));
            if !is_last {
                out.push('\n');
            }
        }
    }

    screen.set_scrollback(original_scrollback);
    (!out.is_empty()).then_some(out)
}

fn clamp_range(
    range: TerminalSelectionRange,
    total_rows: usize,
    cols: u16,
) -> Option<TerminalSelectionRange> {
    if total_rows == 0 {
        return None;
    }
    let last_row = total_rows.saturating_sub(1);
    let start = TerminalSelectionPosition {
        row: range.start.row.min(last_row),
        col: range.start.col.min(cols),
    };
    let end = TerminalSelectionPosition {
        row: range.end.row.min(last_row),
        col: range.end.col.min(cols),
    };
    TerminalSelectionRange::new(start, end)
}

fn make_absolute_row_visible(
    screen: &mut Screen,
    max_scrollback: usize,
    absolute_row: usize,
) -> u16 {
    if absolute_row < max_scrollback {
        screen.set_scrollback(max_scrollback - absolute_row);
        0
    } else {
        screen.set_scrollback(0);
        absolute_row
            .saturating_sub(max_scrollback)
            .min(usize::from(u16::MAX)) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_state_normalizes_reversed_range() {
        let mut state = TerminalSelectionState::default();
        state.start(TerminalSelectionPosition::new(4, 8));
        state.update(TerminalSelectionPosition::new(2, 3));

        assert_eq!(
            state.range(),
            Some(TerminalSelectionRange {
                start: TerminalSelectionPosition::new(2, 3),
                end: TerminalSelectionPosition::new(4, 8),
            })
        );
        assert!(state.clear());
        assert_eq!(state.range(), None);
    }

    #[test]
    fn view_cell_hit_testing_uses_scrollback_absolute_rows() {
        assert_eq!(
            position_for_view_cell(10, 4, 5, 80, 2, 81),
            TerminalSelectionPosition::new(8, 80)
        );
        assert_eq!(
            position_for_view_cell(10, 0, 5, 80, 2, 3),
            TerminalSelectionPosition::new(12, 3)
        );
    }

    #[test]
    fn selection_ranges_expand_wide_character_cells() {
        let mut parser = vt100::Parser::new(2, 10, 0);
        parser.process("alpha 你".as_bytes());
        let range = TerminalSelectionRange {
            start: TerminalSelectionPosition::new(0, 7),
            end: TerminalSelectionPosition::new(0, 8),
        };

        assert_eq!(
            selected_cell_ranges_for_screen_row(parser.screen(), 0, 0, 10, range),
            vec![(6, 8)]
        );
    }

    #[test]
    fn selected_text_uses_vt100_screen_rows_and_wide_chars() {
        let mut parser = vt100::Parser::new(4, 10, 0);
        parser.process("alpha 你\r\nbeta".as_bytes());
        let range = TerminalSelectionRange {
            start: TerminalSelectionPosition::new(0, 6),
            end: TerminalSelectionPosition::new(1, 2),
        };

        assert_eq!(
            selected_text_from_screen(parser.screen_mut(), 0, range).as_deref(),
            Some("你\nbe")
        );
    }
}
