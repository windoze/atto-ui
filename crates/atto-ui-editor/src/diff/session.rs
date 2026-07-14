// Shared, width-aware diff state behind the two side-by-side panes and the unified view.

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::sync::{Arc, Mutex};

use editor_core::{Cell, EditorStateManager, FOLD_PLACEHOLDER_STYLE_ID, StyleId};
use editor_core_diff::{DiffLineKind, LineDiffConfig, diff_line_hunks};
use editor_core_diff_view::{DiffMode, DiffModel, DiffProjection, Row, RowSlot};

use crate::EditorSyntaxConfig;
use crate::syntax::build_syntax_processor;

pub(crate) const BEFORE_SIDE: usize = 0;
pub(crate) const AFTER_SIDE: usize = 1;

/// Shared handle to the diff session. `Component: Send` forbids `Rc`, so the two
/// side-by-side panes share state through an `Arc<Mutex<_>>`.
pub(crate) type SharedSession = Arc<Mutex<DiffSession>>;

/// Width-independent model + the width-dependent projection currently in use, plus the
/// single scroll offset shared by every column (so both sides scroll together).
pub(crate) struct DiffSession {
    model: DiffModel,
    mode: DiffMode,
    line_cfg: LineDiffConfig,
    projection: DiffProjection,
    visible_projection: DiffProjection,
    syntax: EditorSyntaxConfig,
    side_syntax: Vec<SideSyntax>,
    hunks: Vec<DiffHunk>,
    collapsed_hunks: BTreeSet<usize>,
    visible_hunk_rows: Vec<(usize, Range<usize>)>,
    /// Text widths (excluding gutter) used to build `projection`, one per column.
    col_text_widths: Vec<usize>,
    /// Top row on the shared unified row axis.
    scroll_top: usize,
    show_line_numbers: bool,
}

struct SideSyntax {
    state: EditorStateManager,
}

#[derive(Clone, Debug)]
struct DiffHunk {
    before: Range<usize>,
    after: Range<usize>,
}

impl DiffSession {
    pub(crate) fn new(
        before: &str,
        after: &str,
        line_cfg: LineDiffConfig,
        mode: DiffMode,
        show_line_numbers: bool,
        syntax: EditorSyntaxConfig,
    ) -> Self {
        let model = DiffModel::from_before_after(before, after, line_cfg);
        let side_syntax = build_side_syntax(&model, &syntax);
        let hunks = build_hunks(before, after, line_cfg);

        Self {
            model,
            mode,
            line_cfg,
            projection: DiffProjection::default(),
            visible_projection: DiffProjection::default(),
            syntax,
            side_syntax,
            hunks,
            collapsed_hunks: BTreeSet::new(),
            visible_hunk_rows: Vec::new(),
            col_text_widths: Vec::new(),
            scroll_top: 0,
            show_line_numbers,
        }
    }

    pub(crate) fn into_shared(self) -> SharedSession {
        Arc::new(Mutex::new(self))
    }

    pub(crate) fn set_texts(&mut self, before: &str, after: &str) {
        self.model = DiffModel::from_before_after(before, after, self.line_cfg);
        self.side_syntax = build_side_syntax(&self.model, &self.syntax);
        self.hunks = build_hunks(before, after, self.line_cfg);
        self.collapsed_hunks.clear();
        self.invalidate_projection();
        self.scroll_top = 0;
    }

    pub(crate) fn set_syntax(&mut self, syntax: EditorSyntaxConfig) {
        if self.syntax != syntax {
            self.syntax = syntax;
            self.side_syntax = build_side_syntax(&self.model, &self.syntax);
            self.invalidate_projection();
        }
    }

    pub(crate) fn set_mode(&mut self, mode: DiffMode) {
        if self.mode != mode {
            self.mode = mode;
            self.invalidate_projection();
        }
    }

    pub(crate) fn set_show_line_numbers(&mut self, show: bool) {
        self.show_line_numbers = show;
    }

    pub(crate) fn show_line_numbers(&self) -> bool {
        self.show_line_numbers
    }

    fn invalidate_projection(&mut self) {
        self.col_text_widths.clear();
        self.projection = DiffProjection::default();
        self.visible_projection = DiffProjection::default();
        self.visible_hunk_rows.clear();
    }

    pub(crate) fn column_count(&self) -> usize {
        match self.mode {
            DiffMode::Unified => 1,
            DiffMode::SideBySide => 2,
        }
    }

    /// Number of logical lines on a side, used to size line-number gutters stably.
    pub(crate) fn side_line_count(&self, side: usize) -> usize {
        self.model.side(side).map(|s| s.line_count()).unwrap_or(0)
    }

    pub(crate) fn visible_projection(&self) -> &DiffProjection {
        &self.visible_projection
    }

    pub(crate) fn row_count(&self) -> usize {
        self.visible_projection.rows().len()
    }

    pub(crate) fn scroll_top(&self) -> usize {
        self.scroll_top
    }

    pub(crate) fn max_scroll_top(&self, viewport_rows: usize) -> usize {
        self.row_count().saturating_sub(viewport_rows.max(1))
    }

    pub(crate) fn set_scroll_top(&mut self, top: usize, viewport_rows: usize) {
        self.scroll_top = top.min(self.max_scroll_top(viewport_rows));
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, viewport_rows: usize) {
        let next = if delta < 0 {
            self.scroll_top.saturating_sub((-delta) as usize)
        } else {
            self.scroll_top.saturating_add(delta as usize)
        };
        self.set_scroll_top(next, viewport_rows);
    }

    pub(crate) fn toggle_hunk_at_or_after_scroll(&mut self, viewport_rows: usize) -> bool {
        let Some(hunk_index) = self
            .visible_hunk_rows
            .iter()
            .find(|(_, rows)| rows.end > self.scroll_top)
            .map(|(hunk_index, _)| *hunk_index)
        else {
            return false;
        };

        if !self.collapsed_hunks.insert(hunk_index) {
            self.collapsed_hunks.remove(&hunk_index);
        }
        self.rebuild_visible_projection();
        self.set_scroll_top(self.scroll_top, viewport_rows);
        true
    }

    /// Reports the available text width (gutter excluded) for one column. Each pane reports its
    /// own column on draw; the projection is rebuilt once every column width is known and any
    /// changed. The scroll position is preserved across the re-wrap by anchoring on the first
    /// real logical line currently at the top of the viewport — this exercises the diff-view
    /// row mapping that must stay correct when the splitter position changes.
    pub(crate) fn report_column_width(&mut self, column: usize, text_width: usize) {
        let n = self.column_count();
        if self.col_text_widths.len() != n {
            self.col_text_widths = vec![0; n];
        }
        if column >= n || text_width == 0 || self.col_text_widths[column] == text_width {
            return;
        }

        let anchor = self.top_anchor();
        self.col_text_widths[column] = text_width;

        if self.col_text_widths.iter().all(|w| *w > 0) {
            self.projection = DiffProjection::build(&self.model, self.mode, &self.col_text_widths);
            self.apply_syntax_to_projection();
            self.rebuild_visible_projection();
            if let Some((side, logical_line)) = anchor
                && let Some(row) = self.unified_row_for_logical(side, logical_line)
            {
                self.scroll_top = row;
            }
            let max = self.row_count().saturating_sub(1);
            self.scroll_top = self.scroll_top.min(max);
        }
    }

    /// (side, logical_line) of the first real line at/below the current scroll position.
    fn top_anchor(&self) -> Option<(usize, usize)> {
        let rows = self.visible_projection.rows();
        for row in rows.iter().skip(self.scroll_top) {
            for slot in row.slots() {
                if let RowSlot::Line {
                    side, logical_line, ..
                } = slot
                {
                    return Some((*side, *logical_line));
                }
            }
        }
        None
    }

    /// Unified row index of the first segment of `logical_line` on `side`.
    fn unified_row_for_logical(&self, side: usize, logical_line: usize) -> Option<usize> {
        self.visible_projection.rows().iter().position(|row| {
            row.slots().iter().any(|slot| {
                matches!(
                    slot,
                    RowSlot::Line {
                        side: s,
                        logical_line: l,
                        visual_in_logical: 0,
                        ..
                    } if *s == side && *l == logical_line
                )
            })
        })
    }

    fn apply_syntax_to_projection(&mut self) {
        let mut next_col_by_line = HashMap::<(usize, usize), usize>::new();

        for row in &mut self.projection.rows {
            for slot in &mut row.slots {
                let RowSlot::Line {
                    side,
                    logical_line,
                    cells,
                    ..
                } = slot
                else {
                    continue;
                };

                let Some(side_syntax) = self.side_syntax.get(*side) else {
                    continue;
                };
                let col = next_col_by_line.entry((*side, *logical_line)).or_insert(0);
                for cell in cells {
                    for style_id in side_syntax.styles_at(*logical_line, *col) {
                        if !cell.styles.contains(&style_id) {
                            cell.styles.push(style_id);
                        }
                    }
                    *col += 1;
                }
            }
        }
    }

    fn rebuild_visible_projection(&mut self) {
        self.visible_hunk_rows.clear();
        if self.projection.rows.is_empty() {
            self.visible_projection = DiffProjection::default();
            return;
        }

        let hunk_ranges = self.full_projection_hunk_ranges();
        if hunk_ranges.is_empty() {
            self.visible_projection = self.projection.clone();
            return;
        }

        let mut rows = Vec::new();
        let mut hunk_iter = hunk_ranges.into_iter().peekable();
        let mut full_row = 0usize;

        while full_row < self.projection.rows.len() {
            if let Some((hunk_index, range)) = hunk_iter.peek().cloned()
                && full_row == range.start
            {
                let visible_start = rows.len();
                if self.collapsed_hunks.contains(&hunk_index) {
                    rows.push(collapsed_hunk_row(
                        hunk_index,
                        range.end.saturating_sub(range.start),
                        self.projection.columns,
                    ));
                    self.visible_hunk_rows
                        .push((hunk_index, visible_start..visible_start + 1));
                } else {
                    rows.extend(self.projection.rows[range.clone()].iter().cloned());
                    self.visible_hunk_rows
                        .push((hunk_index, visible_start..rows.len()));
                }
                full_row = range.end;
                hunk_iter.next();
                continue;
            }

            rows.push(self.projection.rows[full_row].clone());
            full_row += 1;
        }

        self.visible_projection = DiffProjection {
            columns: self.projection.columns,
            rows,
        };
    }

    fn full_projection_hunk_ranges(&self) -> Vec<(usize, Range<usize>)> {
        let mut ranges = Vec::new();
        for (hunk_index, hunk) in self.hunks.iter().enumerate() {
            let mut start = None;
            let mut end = 0usize;
            for (row_index, row) in self.projection.rows.iter().enumerate() {
                if row_in_hunk(row, hunk) {
                    start.get_or_insert(row_index);
                    end = row_index + 1;
                }
            }
            if let Some(start) = start
                && start < end
            {
                ranges.push((hunk_index, start..end));
            }
        }
        ranges
    }
}

impl SideSyntax {
    fn styles_at(&self, logical_line: usize, column: usize) -> Vec<StyleId> {
        let offset = self
            .state
            .editor()
            .line_index()
            .position_to_char_offset(logical_line, column);
        self.state.get_styles_at(offset)
    }
}

fn build_side_syntax(model: &DiffModel, syntax: &EditorSyntaxConfig) -> Vec<SideSyntax> {
    model
        .sides()
        .iter()
        .map(|side| {
            let mut state = EditorStateManager::new(side.text(), 1);
            if let Some(mut processor) = build_syntax_processor(syntax.clone()) {
                processor.apply(&mut state);
            }
            SideSyntax { state }
        })
        .collect()
}

fn build_hunks(before: &str, after: &str, line_cfg: LineDiffConfig) -> Vec<DiffHunk> {
    diff_line_hunks(before, after, line_cfg)
        .into_iter()
        .map(|hunk| DiffHunk {
            before: hunk.before,
            after: hunk.after,
        })
        .collect()
}

fn row_in_hunk(row: &Row, hunk: &DiffHunk) -> bool {
    row.slots().iter().any(|slot| match slot {
        RowSlot::Line {
            side, logical_line, ..
        } => match *side {
            BEFORE_SIDE => hunk.before.contains(logical_line),
            AFTER_SIDE => hunk.after.contains(logical_line),
            _ => false,
        },
        RowSlot::Spacer { .. } => false,
    })
}

fn collapsed_hunk_row(hunk_index: usize, row_count: usize, columns: usize) -> Row {
    let mut slots = Vec::with_capacity(columns.max(1));
    let label = format!("[+] hunk {} collapsed ({} rows)", hunk_index + 1, row_count);
    for column in 0..columns.max(1) {
        let cells = if column == 0 {
            label
                .chars()
                .map(|ch| Cell::with_styles(ch, 1, vec![FOLD_PLACEHOLDER_STYLE_ID]))
                .collect()
        } else {
            vec![Cell::with_styles(' ', 1, vec![FOLD_PLACEHOLDER_STYLE_ID])]
        };
        slots.push(RowSlot::Spacer {
            change: DiffLineKind::Context,
            gutter: editor_core_diff_view::Gutter::empty(),
            cells,
        });
    }
    Row::new(slots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{TS_STYLE_FUNCTION, TS_STYLE_KEYWORD};

    #[test]
    fn projection_cells_receive_simple_rust_syntax_styles() {
        let text = "fn main() {\n    println!(\"hello\");\n}\n";
        let mut session = DiffSession::new(
            text,
            text,
            LineDiffConfig::default(),
            DiffMode::SideBySide,
            true,
            EditorSyntaxConfig::SimpleRust,
        );

        session.report_column_width(0, 80);
        session.report_column_width(1, 80);

        let slot = session
            .visible_projection()
            .rows()
            .iter()
            .flat_map(|row| row.slots())
            .find_map(|slot| match slot {
                RowSlot::Line { cells, .. } => {
                    let rendered = cells.iter().map(|cell| cell.ch).collect::<String>();
                    (rendered == "fn main() {").then_some(cells)
                }
                RowSlot::Spacer { .. } => None,
            })
            .expect("fn main row should be projected");

        assert!(slot[0].styles.contains(&TS_STYLE_KEYWORD));
        assert!(slot[3].styles.contains(&TS_STYLE_FUNCTION));
    }
}
