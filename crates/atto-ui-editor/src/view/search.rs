// Find/replace UI + match highlighting for `EditorView`.

use atto_ui::text::TextBuffer;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use editor_core::intervals::{Interval, StyleLayerId};
use editor_core::search::{SearchError, SearchOptions, find_all};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::*;

const SEARCH_STYLE_LAYER: StyleLayerId = StyleLayerId::new(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchMode {
    Find,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchField {
    Query,
    Replacement,
}

#[derive(Debug)]
pub(super) struct SearchState {
    mode: Option<SearchMode>,
    field: SearchField,
    query: TextBuffer,
    replacement: TextBuffer,
    options: SearchOptions,
    last_error: Option<String>,
    last_status: Option<String>,
    last_match_count: usize,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            mode: None,
            field: SearchField::Query,
            query: TextBuffer::new(),
            replacement: TextBuffer::new(),
            options: SearchOptions {
                // A more convenient default than `SearchOptions::default()` for interactive find.
                case_sensitive: false,
                whole_word: false,
                regex: false,
            },
            last_error: None,
            last_status: None,
            last_match_count: 0,
        }
    }
}

impl EditorView {
    pub(super) fn search_is_active(&self) -> bool {
        self.search.mode.is_some()
    }

    pub(super) fn search_query_is_empty(&self) -> bool {
        self.search.query.text().is_empty()
    }

    pub(super) fn search_seed_from_selection(&self) -> Option<String> {
        let cursor_state = self.state_manager.get_cursor_state();
        let primary = cursor_state
            .selections
            .get(cursor_state.primary_selection_index)?;
        if primary.start == primary.end {
            return None;
        }
        if primary.start.line != primary.end.line {
            // Multi-line selections are awkward defaults for interactive find.
            return None;
        }

        let (start, end) = self.selection_offsets(primary);
        let len = end.saturating_sub(start);
        if len == 0 || len > 128 {
            return None;
        }

        let text = self.state_manager.editor().get_text();
        let selected = text.chars().skip(start).take(len).collect::<String>();
        if selected.contains('\n') {
            return None;
        }
        Some(selected)
    }

    pub(super) fn search_bar_height(&self) -> u16 {
        match self.search.mode {
            None => 0,
            Some(SearchMode::Find) => 1,
            Some(SearchMode::Replace) => 2,
        }
    }

    pub(super) fn open_find(&mut self, seed: Option<&str>) {
        self.search.mode = Some(SearchMode::Find);
        self.search.field = SearchField::Query;
        self.search.last_status = None;

        if let Some(seed) = seed
            && !seed.is_empty()
        {
            self.search.query.set_text(seed);
        } else if self.search.query.text().is_empty() {
            // Keep it empty; user will type.
        }

        self.apply_search_highlighting();
    }

    pub(super) fn open_replace(&mut self, seed: Option<&str>) {
        self.search.mode = Some(SearchMode::Replace);
        self.search.field = SearchField::Query;
        self.search.last_status = None;

        if let Some(seed) = seed
            && !seed.is_empty()
        {
            self.search.query.set_text(seed);
        }

        self.apply_search_highlighting();
    }

    pub(super) fn close_search(&mut self) {
        self.search.mode = None;
        self.search.last_error = None;
        self.search.last_status = None;
        self.search.last_match_count = 0;
        self.state_manager.clear_style_layer(SEARCH_STYLE_LAYER);
    }

    pub(super) fn handle_search_key_event(&mut self, key: KeyEvent) -> EventResult {
        match key.code {
            KeyCode::Esc => {
                self.close_search();
                return EventResult::consumed();
            }
            KeyCode::Tab => {
                if self.search.mode == Some(SearchMode::Replace) {
                    self.search.field = match self.search.field {
                        SearchField::Query => SearchField::Replacement,
                        SearchField::Replacement => SearchField::Query,
                    };
                }
                return EventResult::consumed();
            }
            KeyCode::Enter => {
                if self.search.query.text().is_empty() {
                    return EventResult::consumed();
                }

                // Shift+Enter: find previous (both Find and Replace).
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.search_find_prev();
                    return EventResult::consumed();
                }

                match self.search.mode {
                    Some(SearchMode::Find) => {
                        self.search_find_next();
                        return EventResult::consumed();
                    }
                    Some(SearchMode::Replace) => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            self.search_replace_all();
                        } else {
                            self.search_replace_current();
                            // Move to the next occurrence so repeated `Enter` behaves naturally.
                            self.search_find_next();
                        }
                        return EventResult::consumed();
                    }
                    None => return EventResult::ignored(),
                }
            }
            KeyCode::Backspace => {
                self.active_search_buffer_mut().backspace();
                self.apply_search_highlighting();
                return EventResult::consumed();
            }
            KeyCode::Delete => {
                self.active_search_buffer_mut().delete();
                self.apply_search_highlighting();
                return EventResult::consumed();
            }
            KeyCode::Left => {
                self.active_search_buffer_mut().move_left();
                return EventResult::consumed();
            }
            KeyCode::Right => {
                self.active_search_buffer_mut().move_right();
                return EventResult::consumed();
            }
            KeyCode::Home => {
                self.active_search_buffer_mut().move_home();
                return EventResult::consumed();
            }
            KeyCode::End => {
                self.active_search_buffer_mut().move_end();
                return EventResult::consumed();
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::ALT => {
                self.search.options.case_sensitive = !self.search.options.case_sensitive;
                self.apply_search_highlighting();
                return EventResult::consumed();
            }
            KeyCode::Char('w') if key.modifiers == KeyModifiers::ALT => {
                self.search.options.whole_word = !self.search.options.whole_word;
                self.apply_search_highlighting();
                return EventResult::consumed();
            }
            KeyCode::Char('r') if key.modifiers == KeyModifiers::ALT => {
                self.search.options.regex = !self.search.options.regex;
                self.apply_search_highlighting();
                return EventResult::consumed();
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.active_search_buffer_mut().insert_char(ch);
                self.apply_search_highlighting();
                return EventResult::consumed();
            }
            _ => {}
        }

        EventResult::ignored()
    }

    fn active_search_buffer_mut(&mut self) -> &mut TextBuffer {
        match self.search.field {
            SearchField::Query => &mut self.search.query,
            SearchField::Replacement => &mut self.search.replacement,
        }
    }

    fn active_search_buffer(&self) -> &TextBuffer {
        match self.search.field {
            SearchField::Query => &self.search.query,
            SearchField::Replacement => &self.search.replacement,
        }
    }

    fn search_query_string(&self) -> String {
        self.search.query.text().to_string()
    }

    fn apply_search_highlighting(&mut self) {
        self.search.last_error = None;
        self.search.last_status = None;
        self.search.last_match_count = 0;

        let query = self.search.query.text();
        if query.is_empty() {
            self.state_manager.clear_style_layer(SEARCH_STYLE_LAYER);
            return;
        }

        let text = self.state_manager.editor().get_text();
        let matches = match find_all(&text, query, self.search.options) {
            Ok(m) => m,
            Err(err) => {
                self.search.last_error = Some(search_error_message(err));
                self.state_manager.clear_style_layer(SEARCH_STYLE_LAYER);
                return;
            }
        };

        self.search.last_match_count = matches.len();

        let intervals = matches
            .into_iter()
            .map(|m| Interval::new(m.start, m.end, crate::theme::SEARCH_MATCH_STYLE_ID))
            .collect::<Vec<_>>();

        self.state_manager
            .replace_style_layer(SEARCH_STYLE_LAYER, intervals);
    }

    pub(super) fn search_find_next(&mut self) {
        let query = self.search_query_string();
        let res = self
            .state_manager
            .execute(Command::Cursor(CursorCommand::FindNext {
                query,
                options: self.search.options,
            }));

        match res {
            Ok(editor_core::CommandResult::SearchNotFound) => {
                self.search.last_status = Some("Not found".to_string());
            }
            Ok(_) => {
                self.search.last_status = None;
                self.adjust_scroll();
            }
            Err(_) => {}
        }
    }

    pub(super) fn search_find_prev(&mut self) {
        let query = self.search_query_string();
        let res = self
            .state_manager
            .execute(Command::Cursor(CursorCommand::FindPrev {
                query,
                options: self.search.options,
            }));

        match res {
            Ok(editor_core::CommandResult::SearchNotFound) => {
                self.search.last_status = Some("Not found".to_string());
            }
            Ok(_) => {
                self.search.last_status = None;
                self.adjust_scroll();
            }
            Err(_) => {}
        }
    }

    fn search_replace_current(&mut self) {
        let query = self.search_query_string();
        let replacement = self.search.replacement.text().to_string();

        let old_char_count = self.state_manager.editor().char_count();
        let mut full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
            lsp.full_document_change(&self.state_manager.editor().line_index, old_char_count, "")
        });

        let before = self.state_manager.editor().get_text();
        let res = self
            .state_manager
            .execute(Command::Edit(EditCommand::ReplaceCurrent {
                query,
                replacement,
                options: self.search.options,
            }));
        if res.is_err() {
            return;
        }

        let after = self.state_manager.editor().get_text();
        if after != before {
            self.config.text.set(after.clone());
            self.maybe_apply_syntax_highlighting();
            self.apply_search_highlighting();

            if let Some(mut change) = full_lsp_change.take() {
                change.text = after;
                self.lsp_did_change(change);
            }
        }
    }

    fn search_replace_all(&mut self) {
        let query = self.search_query_string();
        let replacement = self.search.replacement.text().to_string();

        let old_char_count = self.state_manager.editor().char_count();
        let mut full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
            lsp.full_document_change(&self.state_manager.editor().line_index, old_char_count, "")
        });

        let before = self.state_manager.editor().get_text();
        let res = self
            .state_manager
            .execute(Command::Edit(EditCommand::ReplaceAll {
                query,
                replacement,
                options: self.search.options,
            }));
        if res.is_err() {
            return;
        }

        let after = self.state_manager.editor().get_text();
        if after != before {
            self.config.text.set(after.clone());
            self.maybe_apply_syntax_highlighting();
            self.apply_search_highlighting();

            if let Some(mut change) = full_lsp_change.take() {
                change.text = after;
                self.lsp_did_change(change);
            }
        }
    }

    pub(super) fn render_search_bar(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        focused: bool,
        theme: &EditorTheme,
    ) {
        if !self.search_is_active() || area.height == 0 || area.width == 0 {
            return;
        }

        let block = Block::default().style(theme.popup);
        frame.render_widget(block, area);

        let mut lines: Vec<Line<'static>> = Vec::new();
        match self.search.mode {
            Some(SearchMode::Find) => {
                lines.push(self.render_search_line("Find", self.search.query.text()));
            }
            Some(SearchMode::Replace) => {
                lines.push(self.render_search_line("Find", self.search.query.text()));
                lines.push(self.render_search_line("Repl", self.search.replacement.text()));
            }
            None => {}
        }

        // Append status / option flags to the last line if we have room.
        if let Some(last) = lines.last_mut() {
            let flags = search_flags_text(self.search.options);
            let status = self
                .search
                .last_error
                .as_ref()
                .or(self.search.last_status.as_ref())
                .cloned();

            let meta = if let Some(status) = status {
                format!("  [{flags}]  {status}")
            } else {
                format!("  [{flags}]  {} matches", self.search.last_match_count)
            };

            last.spans.push(Span::styled(meta, theme.gutter));
        }

        frame.render_widget(Paragraph::new(lines).style(theme.popup), area);

        if !focused {
            return;
        }

        // Cursor placement.
        let (row, label) = match (self.search.mode, self.search.field) {
            (Some(SearchMode::Find), _) => (0u16, "Find"),
            (Some(SearchMode::Replace), SearchField::Query) => (0u16, "Find"),
            (Some(SearchMode::Replace), SearchField::Replacement) => (1u16, "Repl"),
            (None, _) => return,
        };

        if row >= area.height {
            return;
        }

        let cursor_col = self.active_search_buffer().cursor_display_col();
        let label_len = label.chars().count() as u16;
        let x = area.x.saturating_add((label_len + 2).min(area.width));
        let x = x
            .saturating_add(cursor_col)
            .min(area.x + area.width.saturating_sub(1));
        let y = area.y.saturating_add(row);
        frame.set_cursor_position((x, y));
    }

    fn render_search_line(&self, label: &str, text: &str) -> Line<'static> {
        let mut spans = Vec::new();
        spans.push(Span::raw(format!("{label}: ")));
        spans.push(Span::raw(text.to_string()));
        Line::from(spans)
    }
}

fn search_flags_text(options: SearchOptions) -> String {
    let mut flags = String::new();
    if options.case_sensitive {
        flags.push('C');
    } else {
        flags.push('c');
    }
    if options.whole_word {
        flags.push('W');
    } else {
        flags.push('w');
    }
    if options.regex {
        flags.push('R');
    } else {
        flags.push('r');
    }
    flags
}

fn search_error_message(err: SearchError) -> String {
    match err {
        SearchError::InvalidRegex(_) => "Invalid regex".to_string(),
    }
}
