// Mouse + selection handling.

use super::*;

impl EditorView {
    fn click_count_for_position(&mut self, pos: Position) -> u8 {
        let now = Instant::now();
        let max_gap = Duration::from_millis(500);

        let next = if let Some(prev) = self.last_click {
            if now.duration_since(prev.at) <= max_gap && prev.pos == pos {
                (prev.count.saturating_add(1)).min(3)
            } else {
                1
            }
        } else {
            1
        };

        self.last_click = Some(ClickState {
            at: now,
            pos,
            count: next,
        });
        next
    }

    fn set_primary_selection(&mut self, start: Position, end: Position) -> bool {
        self.execute(Command::Cursor(CursorCommand::SetSelections {
            selections: vec![Selection {
                start,
                end,
                direction: SelectionDirection::Forward,
            }],
            primary_index: 0,
        }))
    }

    fn select_word_at(&mut self, pos: Position) -> bool {
        let line_text = self
            .state_manager
            .editor()
            .line_index
            .get_line_text(pos.line)
            .unwrap_or_default();
        let chars: Vec<char> = line_text.chars().collect();
        if chars.is_empty() {
            return false;
        }

        let line_len = chars.len();
        let idx = pos.column.min(line_len.saturating_sub(1));

        fn is_word_char(ch: char) -> bool {
            ch.is_alphanumeric() || ch == '_'
        }

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum CharClass {
            Word,
            Whitespace,
            Other,
        }

        fn classify(ch: char) -> CharClass {
            if is_word_char(ch) {
                CharClass::Word
            } else if ch.is_whitespace() {
                CharClass::Whitespace
            } else {
                CharClass::Other
            }
        }

        let cls = classify(chars[idx]);
        let mut start = idx;
        while start > 0 && classify(chars[start - 1]) == cls {
            start -= 1;
        }
        let mut end = idx.saturating_add(1);
        while end < line_len && classify(chars[end]) == cls {
            end += 1;
        }

        self.set_primary_selection(Position::new(pos.line, start), Position::new(pos.line, end))
    }

    fn select_line_at(&mut self, logical_line: usize) -> bool {
        let line_text = self
            .state_manager
            .editor()
            .line_index
            .get_line_text(logical_line)
            .unwrap_or_default();
        let line_char_len = line_text.chars().count();
        self.set_primary_selection(
            Position::new(logical_line, 0),
            Position::new(logical_line, line_char_len),
        )
    }

    pub(super) fn handle_mouse(&mut self, m: MouseEvent) -> EventResult {
        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };
        let Some((local_x, local_y)) = mouse_coords_local_to_area(area, m) else {
            self.mouse_drag = None;
            if self.lsp.hover_anchor.is_some() {
                self.lsp.hover_anchor = None;
            }
            if self.lsp.hover_suppressed_position.is_some() {
                self.lsp.hover_suppressed_position = None;
            }
            self.hide_hover_popup_only();
            return EventResult::ignored();
        };

        let (gutter, text_area) = self.layout_rects(Rect {
            x: 0,
            y: 0,
            width: area.width,
            height: area.height,
        });

        let show_line_numbers = self.config.show_line_numbers.get();
        let show_folding_markers = self.config.show_folding_markers.get();

        // Mouse wheel scrolling should work anywhere inside the editor viewport (including the
        // gutter), not just over the text area.
        match m.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let step = self.scroll_config().wheel_step.max(1) as isize;
                let delta = if matches!(m.kind, MouseEventKind::ScrollUp) {
                    -step
                } else {
                    step
                };
                self.scroll_by_rows(delta);

                // Keep hover anchor reasonably accurate when scrolling over the text area.
                if local_x >= text_area.x && local_y >= text_area.y {
                    let x_in_text = local_x.saturating_sub(text_area.x);
                    let y_in_text = local_y.saturating_sub(text_area.y);
                    if x_in_text < text_area.width && y_in_text < text_area.height {
                        let pos = self.position_at_text_point(x_in_text, y_in_text);
                        let screen = (
                            area.x.saturating_add(local_x),
                            area.y.saturating_add(local_y),
                        );
                        self.update_hover_anchor(pos, screen);
                    }
                }

                return EventResult::consumed();
            }
            _ => {}
        }

        // Folding marker hit testing (gutter-local coordinates).
        if matches!(m.kind, MouseEventKind::Down(MouseButton::Left))
            && show_folding_markers
            && gutter.width > 0
            && local_x < gutter.width.saturating_sub(1)
        {
            let mut x = 0u16;
            let line_count = self.state_manager.editor().line_index.line_count().max(1);
            let digits = line_count.to_string().len().max(2) as u16;
            if show_line_numbers {
                x = x.saturating_add(digits.saturating_add(1));
            }
            // Fold marker column is at x (0-based), plus a space at x+1.
            if local_x == x {
                let scroll_top = self.state_manager.get_viewport_state().scroll_top;
                let visual_row = scroll_top.saturating_add(local_y as usize);
                let (logical_line, visual_in_line) = self
                    .state_manager
                    .editor()
                    .visual_to_logical_line(visual_row);
                if visual_in_line == 0 {
                    self.toggle_fold_at_line(logical_line);
                    return EventResult::consumed();
                }
            }
        }

        // Only handle mouse events within the text area.
        if local_x < text_area.x || local_y < text_area.y {
            self.mouse_drag = None;
            if matches!(m.kind, MouseEventKind::Moved) {
                self.lsp.hover_anchor = None;
                self.lsp.hover_suppressed_position = None;
                self.hide_hover_popup_only();
            }
            return EventResult::ignored();
        }
        let x_in_text = local_x.saturating_sub(text_area.x);
        let y_in_text = local_y.saturating_sub(text_area.y);
        if x_in_text >= text_area.width || y_in_text >= text_area.height {
            self.mouse_drag = None;
            if matches!(m.kind, MouseEventKind::Moved) {
                self.lsp.hover_anchor = None;
                self.lsp.hover_suppressed_position = None;
                self.hide_hover_popup_only();
            }
            return EventResult::ignored();
        }

        match m.kind {
            MouseEventKind::Moved => {
                let pos = self.position_at_text_point(x_in_text, y_in_text);
                let screen = (
                    area.x.saturating_add(local_x),
                    area.y.saturating_add(local_y),
                );
                self.update_hover_anchor(pos, screen);
                EventResult::ignored()
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.hide_popups();
                let pos = self.position_at_text_point(x_in_text, y_in_text);

                let is_shift = m.modifiers.contains(KeyModifiers::SHIFT);
                let is_alt = m.modifiers.contains(KeyModifiers::ALT);
                let is_ctrl = m.modifiers.contains(KeyModifiers::CONTROL);
                let rect_drag = self.rect_selection_mode || (is_alt && is_shift);

                // Double/triple click selection only when no modifier-based mode is active.
                let click_count = if !is_shift && !is_alt && !is_ctrl && !rect_drag {
                    self.click_count_for_position(pos)
                } else {
                    self.last_click = None;
                    1
                };

                if click_count == 2 {
                    if self.select_word_at(pos) {
                        self.adjust_scroll();
                        self.mouse_drag = None;
                        return EventResult::consumed();
                    }
                } else if click_count >= 3 && self.select_line_at(pos.line) {
                    self.adjust_scroll();
                    self.mouse_drag = None;
                    return EventResult::consumed();
                }

                if rect_drag {
                    let _ = self.execute(Command::Cursor(CursorCommand::SetRectSelection {
                        anchor: pos,
                        active: pos,
                    }));
                } else if is_alt || is_ctrl {
                    self.add_secondary_cursor(pos);
                } else if is_shift {
                    let _ =
                        self.execute(Command::Cursor(CursorCommand::ExtendSelection { to: pos }));
                } else {
                    let _ = self.execute(Command::Cursor(CursorCommand::MoveTo {
                        line: pos.line,
                        column: pos.column,
                    }));
                    let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
                    let _ = self.execute(Command::Cursor(CursorCommand::ClearSecondarySelections));
                }

                self.adjust_scroll();
                self.mouse_drag = if (is_alt || is_ctrl) && !rect_drag {
                    None
                } else {
                    Some(MouseDrag {
                        anchor: pos,
                        rect: rect_drag,
                    })
                };
                EventResult::consumed()
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(drag) = self.mouse_drag else {
                    return EventResult::ignored();
                };
                let pos = self.position_at_text_point(x_in_text, y_in_text);

                if drag.rect {
                    let _ = self.execute(Command::Cursor(CursorCommand::SetRectSelection {
                        anchor: drag.anchor,
                        active: pos,
                    }));
                } else {
                    let _ = self.execute(Command::Cursor(CursorCommand::SetSelection {
                        start: drag.anchor,
                        end: pos,
                    }));
                }
                self.adjust_scroll();
                EventResult::consumed()
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_drag = None;
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }

    fn add_secondary_cursor(&mut self, pos: Position) {
        let cursor_state = self.state_manager.get_cursor_state();
        let mut selections = cursor_state.selections;

        // Deduplicate.
        if selections.iter().any(|s| s.start == pos && s.end == pos) {
            return;
        }

        selections.push(Selection {
            start: pos,
            end: pos,
            direction: SelectionDirection::Forward,
        });

        let _ = self.execute(Command::Cursor(CursorCommand::SetSelections {
            selections,
            primary_index: cursor_state.primary_selection_index,
        }));
    }

    fn position_at_text_point(&self, x: u16, y: u16) -> Position {
        let editor = self.state_manager.editor();
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        let visual_row = scroll_top.saturating_add(y as usize);
        let (logical_line, visual_in_line) = editor.visual_to_logical_line(visual_row);

        let Some(layout) = editor.layout_engine.get_line_layout(logical_line) else {
            return Position::new(logical_line, x as usize);
        };

        let line_text = editor
            .line_index
            .get_line_text(logical_line)
            .unwrap_or_default();
        let line_char_len = line_text.chars().count();

        let segment_start_col = if visual_in_line == 0 {
            0
        } else {
            layout
                .wrap_points
                .get(visual_in_line - 1)
                .map(|wp| wp.char_index)
                .unwrap_or(0)
                .min(line_char_len)
        };

        let segment_end_col = layout
            .wrap_points
            .get(visual_in_line)
            .map(|wp| wp.char_index)
            .unwrap_or(line_char_len)
            .min(line_char_len);

        let mut col = segment_start_col;
        let mut cur_x = 0usize;
        for ch in line_text
            .chars()
            .skip(segment_start_col)
            .take(segment_end_col - segment_start_col)
        {
            let w = char_width(ch).max(1);
            if cur_x + w > x as usize {
                break;
            }
            cur_x += w;
            col += 1;
            if cur_x == x as usize {
                break;
            }
        }

        Position::new(logical_line, col.min(segment_end_col))
    }
}

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
