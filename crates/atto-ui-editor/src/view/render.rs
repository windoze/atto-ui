// Rendering helpers for `EditorView`.

use super::*;

impl EditorView {
    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);

        let theme = self.editor_theme();
        frame.render_widget(Block::default().style(theme.background), area);

        let bar_height = self.search_bar_height().min(area.height);
        let content_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.saturating_sub(bar_height),
        };
        let search_area = Rect {
            x: area.x,
            y: area.y + content_area.height,
            width: area.width,
            height: bar_height,
        };

        let (gutter_area, text_area) = self.layout_rects(content_area);
        if text_area.width > 0 && text_area.height > 0 && area.height > 0 && area.width > 0 {
            self.ensure_viewport(text_area);
            self.maybe_request_inlay_hints(ctx.is_focused && !self.search_is_active());

            // Render gutter (line numbers + folding markers).
            if gutter_area.width > 0 {
                self.render_gutter(frame, gutter_area, &theme);
            }

            // Render text viewport.
            self.render_text(
                frame,
                text_area,
                ctx.is_focused && !self.search_is_active(),
                &theme,
            );

            // Inline popups (hover/completion) are rendered on top of the editor viewport.
            self.render_inline_popups(frame, content_area, &theme);
        }

        // Render find/replace bar (and put the cursor there when active).
        if bar_height > 0 {
            self.render_search_bar(frame, search_area, ctx.is_focused, &theme);
        }
    }

    fn render_inline_popups(&mut self, frame: &mut Frame<'_>, bounds: Rect, theme: &EditorTheme) {
        // Interactive popups are mutually exclusive in normal flow; draw them above hover if a
        // stale binding briefly overlaps during event processing.
        self.render_inline_hover_popup(frame, bounds, theme);
        self.render_inline_signature_help_popup(frame, bounds, theme);
        self.render_inline_completion_popup(frame, bounds, theme);
        self.render_inline_code_action_popup(frame, bounds, theme);
    }

    fn render_inline_signature_help_popup(
        &mut self,
        frame: &mut Frame<'_>,
        bounds: Rect,
        theme: &EditorTheme,
    ) {
        let Some(model) = self.signature_help_popup.get() else {
            return;
        };

        let rect = intersect_rect(model.rect, bounds);
        if rect.width == 0 || rect.height == 0 {
            return;
        }

        let active_idx = model.active_signature.unwrap_or(0);
        let Some(signature) = model.signatures.get(active_idx) else {
            return;
        };
        let active_style = theme.popup_selected.add_modifier(Modifier::UNDERLINED);
        let line = crate::popup::signature_help_line(
            signature,
            model.active_parameter,
            theme.popup,
            active_style,
        );

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(theme.popup_border)
            .style(theme.popup);

        frame.render_widget(ratatui::widgets::Clear, rect);
        frame.render_widget(
            Paragraph::new(vec![line]).style(theme.popup).block(block),
            rect,
        );
    }

    fn render_inline_code_action_popup(
        &mut self,
        frame: &mut Frame<'_>,
        bounds: Rect,
        theme: &EditorTheme,
    ) {
        let Some(model) = self.code_action_popup.get() else {
            return;
        };

        let rect = intersect_rect(model.rect, bounds);
        if rect.width == 0 || rect.height == 0 {
            return;
        }

        let inner_height = rect.height.saturating_sub(2) as usize;
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(inner_height);

        for row in 0..inner_height {
            let idx = model.scroll.saturating_add(row);
            if idx >= model.items.len() {
                lines.push(Line::from(""));
                continue;
            }

            let item = &model.items[idx];
            let mut style = theme.popup;
            if idx == model.selected {
                style = theme.popup_selected;
            }
            lines.push(Line::from(Span::styled(
                crate::popup::code_action_line(item, rect.width.saturating_sub(2) as usize),
                style,
            )));
        }

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(theme.popup_border)
            .style(theme.popup);

        frame.render_widget(ratatui::widgets::Clear, rect);
        frame.render_widget(Paragraph::new(lines).style(theme.popup).block(block), rect);
    }

    fn render_inline_hover_popup(
        &mut self,
        frame: &mut Frame<'_>,
        bounds: Rect,
        theme: &EditorTheme,
    ) {
        let Some(model) = self.hover_popup.get() else {
            return;
        };

        let rect = intersect_rect(model.rect, bounds);
        if rect.width == 0 || rect.height == 0 {
            return;
        }

        let lines: Vec<Line<'static>> = model
            .contents
            .lines()
            .iter()
            .map(|l| Line::from(l.clone()))
            .collect();

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(theme.popup_border)
            .style(theme.popup);

        frame.render_widget(ratatui::widgets::Clear, rect);
        frame.render_widget(Paragraph::new(lines).style(theme.popup).block(block), rect);
    }

    fn render_inline_completion_popup(
        &mut self,
        frame: &mut Frame<'_>,
        bounds: Rect,
        theme: &EditorTheme,
    ) {
        let Some(model) = self.completion_popup.get() else {
            return;
        };

        let rect = intersect_rect(model.rect, bounds);
        if rect.width == 0 || rect.height == 0 {
            return;
        }

        let inner_height = rect.height.saturating_sub(2) as usize;
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(inner_height);

        for row in 0..inner_height {
            let idx = model.scroll.saturating_add(row);
            if idx >= model.items.len() {
                lines.push(Line::from(""));
                continue;
            }

            let item = &model.items[idx];
            let mut style = theme.popup;
            if idx == model.selected {
                style = theme.popup_selected;
            }

            let mut line = item.label.clone();
            if let Some(detail) = &item.detail
                && !detail.is_empty()
            {
                line.push_str("  ");
                line.push_str(detail);
            }

            // Hard truncate to avoid excessive allocations for wide terminals.
            let max_w = rect.width.saturating_sub(2) as usize;
            let line = if line.chars().count() > max_w && max_w >= 1 {
                line.chars()
                    .take(max_w.saturating_sub(1))
                    .collect::<String>()
                    + "…"
            } else {
                line
            };

            lines.push(Line::from(Span::styled(line, style)));
        }

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(theme.popup_border)
            .style(theme.popup);

        frame.render_widget(ratatui::widgets::Clear, rect);
        frame.render_widget(Paragraph::new(lines).style(theme.popup).block(block), rect);
    }

    fn render_gutter(&self, frame: &mut Frame<'_>, area: Rect, theme: &EditorTheme) {
        let show_line_numbers = self.config.show_line_numbers.get();
        let show_folding_markers = self.config.show_folding_markers.get();
        let editor = self.state_manager.editor();
        let show_diagnostics = !editor.diagnostics().is_empty();
        if !show_line_numbers && !show_folding_markers && !show_diagnostics {
            return;
        }

        let cursor_line = self.state_manager.get_cursor_state().position.line;
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;

        let line_count = editor.line_index().line_count().max(1);
        let digits = line_count.to_string().len().max(2);

        // Glyph shown in the folding-marker column for each logical line.
        // Precedence (highest last): middle connector `│` < end `▲` < start (`▶`/`▼`).
        let mut fold_glyph = std::collections::HashMap::<usize, char>::new();
        for r in editor.folding_manager().regions() {
            if r.is_collapsed {
                continue;
            }
            for line in (r.start_line + 1)..r.end_line {
                fold_glyph.insert(line, '│');
            }
        }
        for r in editor.folding_manager().regions() {
            if !r.is_collapsed {
                fold_glyph.insert(r.end_line, '▲');
            }
        }
        for r in editor.folding_manager().regions() {
            fold_glyph.insert(r.start_line, if r.is_collapsed { '▶' } else { '▼' });
        }

        // Innermost (smallest) fold region containing the cursor; its connector
        // line and both end triangles are highlighted in the gutter.
        let mut active_region: Option<(usize, usize)> = None;
        for r in editor.folding_manager().regions() {
            if r.start_line <= cursor_line && cursor_line <= r.end_line {
                let span = r.end_line - r.start_line;
                if active_region.map_or(true, |(s, e)| span < e - s) {
                    active_region = Some((r.start_line, r.end_line));
                }
            }
        }

        let mut diagnostic_by_line = std::collections::HashMap::<usize, DiagnosticSeverity>::new();
        if show_diagnostics {
            let line_index = editor.line_index();
            for diagnostic in editor.diagnostics() {
                let (line, _col) = line_index.char_offset_to_position(diagnostic.range.start);
                let severity = diagnostic
                    .severity
                    .unwrap_or(DiagnosticSeverity::Information);
                diagnostic_by_line
                    .entry(line)
                    .and_modify(|existing| {
                        if diagnostic_severity_rank(severity) < diagnostic_severity_rank(*existing)
                        {
                            *existing = severity;
                        }
                    })
                    .or_insert(severity);
            }
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

            let mut spans = Vec::<Span<'static>>::new();
            let mut base = String::new();
            let mut used = 0usize;

            if show_line_numbers {
                if is_wrapped {
                    base.push_str(&" ".repeat(digits + 1));
                } else {
                    base.push_str(&format!("{:>width$} ", logical_line + 1, width = digits));
                }
            }

            used += base.chars().count();
            if !base.is_empty() {
                spans.push(Span::styled(base, gutter_style));
            }

            if show_folding_markers {
                let glyph = if is_wrapped {
                    None
                } else {
                    fold_glyph.get(&logical_line).copied()
                };
                if let Some(g) = glyph {
                    let in_active_region = active_region
                        .map(|(s, e)| s <= logical_line && logical_line <= e)
                        .unwrap_or(false);
                    let marker_style = if in_active_region {
                        theme.fold_marker.add_modifier(Modifier::BOLD)
                    } else {
                        gutter_style
                    };
                    spans.push(Span::styled(g.to_string(), marker_style));
                    spans.push(Span::styled(" ".to_string(), gutter_style));
                } else {
                    spans.push(Span::styled("  ".to_string(), gutter_style));
                }
                used += 2;
            }

            if show_diagnostics {
                if is_wrapped {
                    spans.push(Span::styled("  ".to_string(), gutter_style));
                    used += 2;
                } else if let Some(severity) = diagnostic_by_line.get(&logical_line).copied() {
                    spans.push(Span::styled(
                        diagnostic_marker(severity).to_string(),
                        diagnostic_style(theme, severity),
                    ));
                    spans.push(Span::styled(" ".to_string(), gutter_style));
                    used += 2;
                } else {
                    spans.push(Span::styled("  ".to_string(), gutter_style));
                    used += 2;
                }
            }

            // Separator at the end of gutter (if any).
            if area.width > 0 {
                // Ensure the separator is present even if gutter content is shorter.
                let expected_w = area.width.saturating_sub(1) as usize;
                if used < expected_w {
                    spans.push(Span::styled(" ".repeat(expected_w - used), gutter_style));
                }
                spans.push(Span::styled("│".to_string(), gutter_style));
            }

            lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(lines).style(theme.gutter), area);
    }

    fn style_for_style_ids(&self, style_ids: &[u32]) -> Style {
        let theme = self.editor_theme();

        let mut fg = None;
        let mut bg = None;
        let mut mods = Modifier::empty();

        for &style_id in style_ids {
            if let Some(style) = theme.style_ids.get(&style_id) {
                if style.fg.is_some() {
                    fg = style.fg;
                }
                if style.bg.is_some() {
                    bg = style.bg;
                }
                mods |= style.add_modifier;
                continue;
            }

            // Sublime: map StyleId -> scope string (if configured).
            if let Some(scope) = self
                .syntax_processor
                .as_ref()
                .and_then(|p| p.sublime_scope_for_style_id(style_id))
            {
                if let Some(style) = crate::syntax::style_for_sublime_scope(&theme, scope) {
                    if style.fg.is_some() {
                        fg = style.fg;
                    }
                    if style.bg.is_some() {
                        bg = style.bg;
                    }
                    mods |= style.add_modifier;
                    continue;
                }

                mods |= theme.unknown_scope.add_modifier;
                if theme.unknown_scope.fg.is_some() {
                    fg = theme.unknown_scope.fg;
                }
                if theme.unknown_scope.bg.is_some() {
                    bg = theme.unknown_scope.bg;
                }
                continue;
            }

            // LSP semantic tokens default encoding: high 16 bits token_type, low 16 bits modifiers.
            // Since editor-core 0.4 the indices are canonical (server-legend independent), so we
            // resolve names against the canonical token type/modifier lists rather than the
            // server legend.
            if style_id < 0x0100_0000 {
                let (token_type_idx, token_mod_bits) =
                    editor_core_lsp::decode_semantic_style_id(style_id);
                let token_type_name = editor_core_lsp::CANONICAL_SEMANTIC_TOKEN_TYPES
                    .get(token_type_idx as usize)
                    .copied();

                let token_style = token_type_name
                    .and_then(|name| theme.semantic_tokens.token_types.get(name))
                    .copied()
                    .unwrap_or(theme.semantic_tokens.unknown_token_type);

                if token_style.fg.is_some() {
                    fg = token_style.fg;
                }
                if token_style.bg.is_some() {
                    bg = token_style.bg;
                }
                mods |= token_style.add_modifier;

                for (i, name) in editor_core_lsp::CANONICAL_SEMANTIC_TOKEN_MODIFIERS
                    .iter()
                    .enumerate()
                {
                    if i >= 16 {
                        break;
                    }
                    if token_mod_bits & (1u32 << i) == 0 {
                        continue;
                    }
                    mods |= theme
                        .semantic_tokens
                        .token_modifiers
                        .get(*name)
                        .copied()
                        .unwrap_or(theme.semantic_tokens.unknown_token_modifier);
                }

                continue;
            }

            // Unknown StyleId.
            if theme.unknown_style_id.fg.is_some() {
                fg = theme.unknown_style_id.fg;
            }
            if theme.unknown_style_id.bg.is_some() {
                bg = theme.unknown_style_id.bg;
            }
            mods |= theme.unknown_style_id.add_modifier;
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

    fn render_text(&self, frame: &mut Frame<'_>, area: Rect, focused: bool, theme: &EditorTheme) {
        let editor = self.state_manager.editor();
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        let total_visual = editor.visual_line_count();

        let cursor_state = self.state_manager.get_cursor_state();
        let selections = cursor_state.selections;

        let use_composed = self.config.inlay_hints.enabled.get();
        let styled_grid = (!use_composed).then(|| {
            self.state_manager
                .get_viewport_content_styled(scroll_top, area.height as usize)
        });
        let composed_grid = use_composed.then(|| {
            self.state_manager
                .get_viewport_content_composed(scroll_top, area.height as usize)
        });

        let mut display_lines = Vec::<Line<'static>>::with_capacity(area.height as usize);
        let selection_offset_ranges = if use_composed {
            let line_index = editor.line_index();
            selections
                .iter()
                .filter(|selection| selection.start != selection.end)
                .map(|selection| {
                    let start = line_index
                        .position_to_char_offset(selection.start.line, selection.start.column);
                    let end = line_index
                        .position_to_char_offset(selection.end.line, selection.end.column);
                    (start.min(end), start.max(end))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        for i in 0..(area.height as usize) {
            if area.width == 0 {
                display_lines.push(Line::from(""));
                continue;
            }

            if let Some(grid) = composed_grid.as_ref() {
                let Some(composed_line) = grid.lines.get(i) else {
                    display_lines.push(Line::from(""));
                    continue;
                };
                if composed_line.cells.is_empty() {
                    display_lines.push(Line::from(""));
                    continue;
                }

                let mut spans: Vec<Span<'static>> = Vec::new();
                let mut current_style: Option<Style> = None;
                let mut buffer = String::new();

                for cell in &composed_line.cells {
                    let mut style = self.style_for_style_ids(&cell.styles);
                    if let ComposedCellSource::Document { offset } = cell.source
                        && selection_offset_ranges
                            .iter()
                            .any(|(start, end)| offset >= *start && offset < *end)
                    {
                        style = theme.selection;
                    }
                    push_rendered_cell(
                        &mut spans,
                        &mut current_style,
                        &mut buffer,
                        cell.ch,
                        style,
                        theme,
                    );
                }

                flush_rendered_cells(&mut spans, &mut current_style, &mut buffer, theme);
                display_lines.push(Line::from(spans));
                continue;
            }

            let visual_row = scroll_top + i;
            if visual_row >= total_visual {
                display_lines.push(Line::from(""));
                continue;
            }

            let (logical_line, visual_in_line) = editor.visual_to_logical_line(visual_row);
            let Some(layout) = editor.layout_engine().get_line_layout(logical_line) else {
                display_lines.push(Line::from(""));
                continue;
            };

            let line_text = editor
                .line_index()
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

            let Some(headless_line) = styled_grid.as_ref().and_then(|grid| grid.lines.get(i))
            else {
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

                push_rendered_cell(
                    &mut spans,
                    &mut current_style,
                    &mut buffer,
                    cell.ch,
                    style,
                    theme,
                );
            }

            flush_rendered_cells(&mut spans, &mut current_style, &mut buffer, theme);
            display_lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(display_lines).style(theme.text), area);

        if focused && let Some(Some((cursor_x, cursor_y))) = self.cursor_screen_position() {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn push_rendered_cell(
    spans: &mut Vec<Span<'static>>,
    current_style: &mut Option<Style>,
    buffer: &mut String,
    ch: char,
    style: Style,
    theme: &EditorTheme,
) {
    if current_style.is_none() {
        *current_style = Some(style);
    }
    if *current_style != Some(style) {
        if !buffer.is_empty() {
            spans.push(Span::styled(
                std::mem::take(buffer),
                (*current_style).unwrap_or(theme.text),
            ));
        }
        *current_style = Some(style);
    }
    buffer.push(ch);
}

fn flush_rendered_cells(
    spans: &mut Vec<Span<'static>>,
    current_style: &mut Option<Style>,
    buffer: &mut String,
    theme: &EditorTheme,
) {
    if !buffer.is_empty() {
        spans.push(Span::styled(
            std::mem::take(buffer),
            (*current_style).unwrap_or(theme.text),
        ));
    }
}

fn diagnostic_severity_rank(severity: DiagnosticSeverity) -> u8 {
    match severity {
        DiagnosticSeverity::Error => 0,
        DiagnosticSeverity::Warning => 1,
        DiagnosticSeverity::Information => 2,
        DiagnosticSeverity::Hint => 3,
    }
}

fn diagnostic_marker(severity: DiagnosticSeverity) -> char {
    match severity {
        DiagnosticSeverity::Error => 'E',
        DiagnosticSeverity::Warning => 'W',
        DiagnosticSeverity::Information => 'I',
        DiagnosticSeverity::Hint => 'H',
    }
}

fn diagnostic_style(theme: &EditorTheme, severity: DiagnosticSeverity) -> Style {
    match severity {
        DiagnosticSeverity::Error => theme.diagnostic_error,
        DiagnosticSeverity::Warning => theme.diagnostic_warning,
        DiagnosticSeverity::Information => theme.diagnostic_info,
        DiagnosticSeverity::Hint => theme.diagnostic_hint,
    }
}

fn intersect_rect(a: Rect, b: Rect) -> Rect {
    if a.width == 0 || a.height == 0 || b.width == 0 || b.height == 0 {
        return Rect::default();
    }

    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = a.x.saturating_add(a.width).min(b.x.saturating_add(b.width));
    let y2 =
        a.y.saturating_add(a.height)
            .min(b.y.saturating_add(b.height));

    if x2 <= x1 || y2 <= y1 {
        return Rect::default();
    }

    Rect {
        x: x1,
        y: y1,
        width: x2 - x1,
        height: y2 - y1,
    }
}
