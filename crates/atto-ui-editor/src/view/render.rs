// Rendering helpers for `EditorView`.

use super::*;

impl EditorView {
    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);

        let theme = self.editor_theme();
        frame.render_widget(Block::default().style(theme.background), area);

        let (gutter_area, text_area) = self.layout_rects(area);
        if text_area.width == 0 || text_area.height == 0 || area.height == 0 || area.width == 0 {
            return;
        }

        self.ensure_viewport(text_area);

        // Render gutter (line numbers + folding markers).
        if gutter_area.width > 0 {
            self.render_gutter(frame, gutter_area, &theme);
        }

        // Render text viewport.
        self.render_text(frame, text_area, ctx.is_focused, &theme);
    }

    fn render_gutter(&self, frame: &mut Frame<'_>, area: Rect, theme: &EditorTheme) {
        let show_line_numbers = self.config.show_line_numbers.get();
        let show_folding_markers = self.config.show_folding_markers.get();
        if !show_line_numbers && !show_folding_markers {
            return;
        }

        let editor = self.state_manager.editor();
        let cursor_line = self.state_manager.get_cursor_state().position.line;
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;

        let line_count = editor.line_index.line_count().max(1);
        let digits = line_count.to_string().len().max(2);

        let mut fold_regions_by_start =
            std::collections::HashMap::<usize, editor_core::intervals::FoldRegion>::new();
        for r in editor.folding_manager.regions() {
            fold_regions_by_start
                .entry(r.start_line)
                .or_insert_with(|| r.clone());
        }

        let mut lines = Vec::<Line<'static>>::with_capacity(area.height as usize);
        for row in 0..(area.height as usize) {
            let visual_row = scroll_top.saturating_add(row);
            if visual_row >= editor.visual_line_count() {
                lines.push(Line::from(""));
                continue;
            }

            let (logical_line, visual_in_line) = editor.visual_to_logical_line(visual_row);
            let is_wrapped = visual_in_line > 0;

            let active = logical_line == cursor_line && !is_wrapped;
            let gutter_style = if active {
                theme.gutter_active
            } else {
                theme.gutter
            };

            let mut s = String::new();

            if show_line_numbers {
                if is_wrapped {
                    s.push_str(&" ".repeat(digits + 1));
                } else {
                    s.push_str(&format!("{:>width$} ", logical_line + 1, width = digits));
                }
            }

            if show_folding_markers {
                if is_wrapped {
                    s.push_str("  ");
                } else if let Some(region) = fold_regions_by_start.get(&logical_line) {
                    if region.is_collapsed {
                        s.push('▶');
                    } else {
                        s.push('▼');
                    }
                    s.push(' ');
                } else {
                    s.push_str("  ");
                }
            }

            // Separator at the end of gutter (if any).
            if area.width > 0 {
                // Ensure the separator is present even if gutter content is shorter.
                let expected_w = area.width.saturating_sub(1) as usize;
                if s.chars().count() < expected_w {
                    s.push_str(&" ".repeat(expected_w - s.chars().count()));
                }
                s.push('│');
            }

            lines.push(Line::from(Span::styled(s, gutter_style)));
        }

        frame.render_widget(Paragraph::new(lines).style(theme.gutter), area);
    }

    fn render_text(&self, frame: &mut Frame<'_>, area: Rect, focused: bool, theme: &EditorTheme) {
        let editor = self.state_manager.editor();
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        let total_visual = editor.visual_line_count();

        let cursor_state = self.state_manager.get_cursor_state();
        let selections = cursor_state.selections;

        let grid = self
            .state_manager
            .get_viewport_content_styled(scroll_top, area.height as usize);

        let mut display_lines = Vec::<Line<'static>>::with_capacity(area.height as usize);

        for i in 0..(area.height as usize) {
            if area.width == 0 {
                display_lines.push(Line::from(""));
                continue;
            }
            let visual_row = scroll_top + i;
            if visual_row >= total_visual {
                display_lines.push(Line::from(""));
                continue;
            }

            let (logical_line, visual_in_line) = editor.visual_to_logical_line(visual_row);
            let Some(layout) = editor.layout_engine.get_line_layout(logical_line) else {
                display_lines.push(Line::from(""));
                continue;
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

            let mut selection_ranges: Vec<(usize, usize)> = Vec::new();
            for selection in &selections {
                if selection.start == selection.end {
                    continue;
                }

                let (sel_start, sel_end) = if selection.start <= selection.end {
                    (selection.start, selection.end)
                } else {
                    (selection.end, selection.start)
                };

                if logical_line < sel_start.line || logical_line > sel_end.line {
                    continue;
                }

                let start_col = if logical_line == sel_start.line {
                    sel_start.column.min(line_char_len)
                } else {
                    0
                };
                let end_col = if logical_line == sel_end.line {
                    sel_end.column.min(line_char_len)
                } else {
                    line_char_len
                };
                if start_col < end_col {
                    selection_ranges.push((start_col, end_col));
                }
            }

            let Some(headless_line) = grid.lines.get(i) else {
                display_lines.push(Line::from(""));
                continue;
            };
            if headless_line.cells.is_empty() {
                display_lines.push(Line::from(""));
                continue;
            }

            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut current_style: Option<Style> = None;
            let mut buffer = String::new();

            for (cell_idx, cell) in headless_line.cells.iter().enumerate() {
                let col = segment_start_col + cell_idx;
                let mut style = self.style_for_style_ids(&cell.styles);
                let is_selected = selection_ranges
                    .iter()
                    .any(|(start, end)| col >= *start && col < *end);
                if is_selected {
                    style = theme.selection;
                }

                if current_style.is_none() {
                    current_style = Some(style);
                }
                if current_style != Some(style) {
                    spans.push(Span::styled(
                        std::mem::take(&mut buffer),
                        current_style.unwrap_or(theme.text),
                    ));
                    current_style = Some(style);
                }
                buffer.push(cell.ch);
            }

            if !buffer.is_empty() {
                spans.push(Span::styled(buffer, current_style.unwrap_or(theme.text)));
            }
            display_lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(display_lines).style(theme.text), area);

        if focused && let Some(Some((cursor_x, cursor_y))) = self.cursor_screen_position() {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}
