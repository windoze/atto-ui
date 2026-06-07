// Viewport, scrolling, folding, and cursor movement helpers for `EditorView`.

use super::*;

impl EditorView {
    pub(super) fn ensure_viewport(&mut self, text_area: Rect) {
        let viewport_changed = self.viewport_size != (text_area.width, text_area.height);

        let viewport_height = text_area.height as usize;
        self.state_manager.set_viewport_height(viewport_height);

        let viewport_width = text_area.width.max(1) as usize;
        if viewport_width != self.state_manager.editor().viewport_width() {
            let _ = self
                .state_manager
                .execute(Command::View(ViewCommand::SetViewportWidth {
                    width: viewport_width,
                }));
        }

        if viewport_height > 0 {
            let max_scroll_top = self.max_scroll_top(viewport_height);
            let current_scroll_top = self.state_manager.get_viewport_state().scroll_top;
            if current_scroll_top > max_scroll_top {
                self.state_manager.set_scroll_top(max_scroll_top);
            }
            // Auto-scroll to keep the cursor visible when the viewport geometry changes
            // (e.g. terminal resize). When the viewport is stable, allow user-driven scrolling
            // (mouse wheel / scrollbars) without snapping back to the cursor every frame.
            if viewport_changed {
                self.adjust_scroll();
            }
        }

        self.viewport_size = (text_area.width, text_area.height);
        let total_visual = self.state_manager.editor().visual_line_count();
        self.content_size = (text_area.width, total_visual.min(u16::MAX as usize) as u16);
    }

    pub(super) fn max_scroll_top(&self, viewport_height: usize) -> usize {
        let total_visual = self.state_manager.editor().visual_line_count();
        total_visual.saturating_sub(viewport_height)
    }

    pub(super) fn adjust_scroll(&mut self) {
        let viewport_height = self.state_manager.get_viewport_state().height.unwrap_or(0);
        if viewport_height == 0 {
            return;
        }

        let editor = self.state_manager.editor();
        let cursor_pos = self.active_cursor_position();
        let Some((cursor_visual_row, _)) =
            editor.logical_position_to_visual(cursor_pos.line, cursor_pos.column)
        else {
            return;
        };

        let mut scroll_top = self.state_manager.get_viewport_state().scroll_top;
        if cursor_visual_row < scroll_top {
            scroll_top = cursor_visual_row;
        }
        if cursor_visual_row >= scroll_top + viewport_height {
            scroll_top = cursor_visual_row
                .saturating_sub(viewport_height)
                .saturating_add(1);
        }

        scroll_top = scroll_top.min(self.max_scroll_top(viewport_height));
        self.state_manager.set_scroll_top(scroll_top);
    }

    pub(super) fn toggle_fold_at_cursor(&mut self) {
        let line = self.active_cursor_position().line;
        self.toggle_fold_at_line(line);
    }

    pub(super) fn toggle_fold_at_line(&mut self, logical_line: usize) {
        // LSP and syntax providers populate `folding_manager` with *possible* fold regions
        // (`is_collapsed = false`). When the user folds/unfolds, we should toggle that region
        // in-place rather than adding a duplicate folded region (which makes clicking the gutter
        // marker unable to reliably unfold).
        let mut regions = self
            .state_manager
            .editor()
            .folding_manager()
            .regions()
            .to_vec();

        // Match `editor_core::FoldingManager::toggle_region_starting_at_line`: among all regions
        // starting at this line, choose the innermost one (smallest `end_line`).
        let mut best_idx = None::<usize>;
        let mut best_end = usize::MAX;
        for (idx, region) in regions.iter().enumerate() {
            if region.start_line != logical_line {
                continue;
            }
            if region.end_line <= region.start_line {
                continue;
            }
            if region.end_line < best_end {
                best_end = region.end_line;
                best_idx = Some(idx);
            }
        }

        let Some(idx) = best_idx else {
            return;
        };

        regions[idx].is_collapsed = !regions[idx].is_collapsed;
        self.state_manager.replace_folding_regions(regions, false);
        self.adjust_scroll();
    }

    pub(super) fn unfold_all(&mut self) {
        let _ = self.execute(Command::Style(StyleCommand::UnfoldAll));
    }

    pub(super) fn clear_secondary_selections(&mut self) {
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSecondarySelections));
    }

    pub(super) fn toggle_rect_selection(&mut self) {
        self.rect_selection_mode = !self.rect_selection_mode;
        self.rect_selection_anchor = None;
        self.mouse_drag = None;
        self.last_insert_time = None;
    }

    pub(super) fn maybe_end_undo_group_after_idle(&mut self) {
        let Some(last_insert_time) = self.last_insert_time else {
            return;
        };
        if last_insert_time.elapsed() < Duration::from_millis(750) {
            return;
        }
        if self
            .state_manager
            .get_undo_redo_state()
            .current_change_group
            .is_none()
        {
            self.last_insert_time = None;
            return;
        }
        let _ = self.execute(Command::Edit(EditCommand::EndUndoGroup));
        self.last_insert_time = None;
    }

    pub(super) fn move_cursor(&mut self, delta_line: isize, delta_column: isize, extend: bool) {
        // Clamp positive line movements to avoid `editor_core` rejecting out-of-bounds `MoveBy` /
        // `ExtendSelection` targets (notably `PageDown` near EOF).
        let line_count = self.state_manager.editor().line_index().line_count();
        if line_count == 0 {
            return;
        }
        let last_line = line_count.saturating_sub(1);

        if extend {
            let pos = self.active_cursor_position();
            let target_line = if delta_line.is_negative() {
                pos.line.saturating_sub((-delta_line) as usize)
            } else {
                pos.line.saturating_add(delta_line as usize).min(last_line)
            };
            let target_col = if delta_column.is_negative() {
                pos.column.saturating_sub((-delta_column) as usize)
            } else {
                pos.column.saturating_add(delta_column as usize)
            };
            let target = Position::new(target_line, target_col);
            if self.rect_selection_mode {
                let anchor = self.rect_selection_anchor.unwrap_or(pos);
                if self.rect_selection_anchor.is_none() {
                    self.rect_selection_anchor = Some(anchor);
                }
                let _ = self.execute(Command::Cursor(CursorCommand::SetRectSelection {
                    anchor,
                    active: target,
                }));
            } else {
                self.rect_selection_anchor = None;
                let _ = self.execute(Command::Cursor(CursorCommand::ExtendSelection {
                    to: target,
                }));
            }
        } else {
            // If there is an active primary selection, collapse it first (VSCode-like).
            let cursor_state = self.state_manager.get_cursor_state();
            if let Some(primary) = cursor_state
                .selections
                .get(cursor_state.primary_selection_index)
                && primary.start != primary.end
            {
                let (min_pos, max_pos) = if primary.start <= primary.end {
                    (primary.start, primary.end)
                } else {
                    (primary.end, primary.start)
                };

                let collapse_to = if delta_line.is_negative() || delta_column.is_negative() {
                    min_pos
                } else {
                    max_pos
                };

                let _ = self.execute(Command::Cursor(CursorCommand::MoveTo {
                    line: collapse_to.line,
                    column: collapse_to.column,
                }));
                let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
                self.rect_selection_anchor = None;
                self.adjust_scroll();
                self.hide_popups();
                return;
            }

            let pos = self.active_cursor_position();
            let max_down = last_line.saturating_sub(pos.line) as isize;
            let delta_line = if delta_line.is_positive() {
                delta_line.min(max_down)
            } else {
                delta_line
            };

            let _ = self.execute(Command::Cursor(CursorCommand::MoveBy {
                delta_line,
                delta_column,
            }));
            let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
            self.rect_selection_anchor = None;
        }
        self.adjust_scroll();
        self.hide_popups();
    }

    pub(super) fn move_home_end(&mut self, end: bool, extend: bool) {
        let editor = self.state_manager.editor();
        let pos = self.active_cursor_position();
        let line_text = editor
            .line_index()
            .get_line_text(pos.line)
            .unwrap_or_default();
        let col = if end { line_text.chars().count() } else { 0 };
        let target = Position::new(pos.line, col);

        if extend {
            let _ = self.execute(Command::Cursor(CursorCommand::ExtendSelection {
                to: target,
            }));
        } else {
            let _ = self.execute(Command::Cursor(CursorCommand::MoveTo {
                line: target.line,
                column: target.column,
            }));
            let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
        }
        self.adjust_scroll();
        self.hide_popups();
    }

    pub(super) fn page_scroll(&mut self, down: bool, extend: bool) {
        let viewport_height = self.state_manager.get_viewport_state().height.unwrap_or(0);
        if viewport_height == 0 {
            return;
        }
        let delta = if down {
            viewport_height as isize
        } else {
            -(viewport_height as isize)
        };
        self.move_cursor(delta, 0, extend);
    }

    pub(super) fn scroll_by_rows(&mut self, delta_rows: isize) {
        let viewport_height = self.state_manager.get_viewport_state().height.unwrap_or(0);
        if viewport_height == 0 {
            return;
        }
        let mut scroll_top = self.state_manager.get_viewport_state().scroll_top;
        if delta_rows.is_positive() {
            scroll_top = scroll_top.saturating_add(delta_rows as usize);
        } else {
            scroll_top = scroll_top.saturating_sub((-delta_rows) as usize);
        }
        scroll_top = scroll_top.min(self.max_scroll_top(viewport_height));
        self.state_manager.set_scroll_top(scroll_top);
    }
}
