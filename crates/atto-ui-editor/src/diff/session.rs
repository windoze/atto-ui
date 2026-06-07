// Shared, width-aware diff state behind the two side-by-side panes and the unified view.

use std::sync::{Arc, Mutex};

use editor_core_diff::LineDiffConfig;
use editor_core_diff_view::{DiffMode, DiffModel, DiffProjection, RowSlot};

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
    /// Text widths (excluding gutter) used to build `projection`, one per column.
    col_text_widths: Vec<usize>,
    /// Top row on the shared unified row axis.
    scroll_top: usize,
    show_line_numbers: bool,
}

impl DiffSession {
    pub(crate) fn new(
        before: &str,
        after: &str,
        line_cfg: LineDiffConfig,
        mode: DiffMode,
        show_line_numbers: bool,
    ) -> Self {
        Self {
            model: DiffModel::from_before_after(before, after, line_cfg),
            mode,
            line_cfg,
            projection: DiffProjection::default(),
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
        self.invalidate_projection();
        self.scroll_top = 0;
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

    pub(crate) fn projection(&self) -> &DiffProjection {
        &self.projection
    }

    pub(crate) fn row_count(&self) -> usize {
        self.projection.rows().len()
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
        let rows = self.projection.rows();
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
        self.projection.rows().iter().position(|row| {
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
}
