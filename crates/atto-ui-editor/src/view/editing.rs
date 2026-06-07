// Text editing, clipboard, and indentation operations for `EditorView`.

use super::*;

impl EditorView {
    pub(super) fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let has_multi = !self
            .state_manager
            .editor()
            .secondary_selections()
            .is_empty();

        let mut full_lsp_change = None::<LspContentChange>;
        let mut lsp_change = None::<LspContentChange>;
        if let Some(lsp) = self.lsp.session.as_ref() {
            if has_multi {
                let old_char_count = self.state_manager.editor().char_count();
                full_lsp_change = Some(lsp.full_document_change(
                    self.state_manager.editor().line_index(),
                    old_char_count,
                    "",
                ));
            } else {
                let cursor_state = self.state_manager.get_cursor_state();
                let primary = cursor_state.primary_selection_index;
                let selection = cursor_state.selections.get(primary);
                let (start, end) = selection
                    .filter(|s| s.start != s.end)
                    .map(|s| self.selection_offsets(s))
                    .unwrap_or_else(|| {
                        let offset = self.cursor_offset();
                        (offset, offset)
                    });
                lsp_change = Some(lsp.content_change_for_offsets(
                    self.state_manager.editor().line_index(),
                    start,
                    end,
                    text,
                ));
            }
        }

        if !self.execute_and_sync_text(Command::Edit(EditCommand::InsertText {
            text: text.to_string(),
        })) {
            return;
        }

        self.last_insert_time = Some(Instant::now());
        self.maybe_apply_syntax_highlighting();
        self.hide_hover_popup_only();

        if let Some(change) = lsp_change {
            self.lsp_did_change(change);
        } else if let Some(mut change) = full_lsp_change {
            change.text = self.state_manager.editor().get_text();
            self.lsp_did_change(change);
        }
    }

    pub(super) fn backspace(&mut self) {
        let has_multi = !self
            .state_manager
            .editor()
            .secondary_selections()
            .is_empty();

        let cursor_state = self.state_manager.get_cursor_state();
        let has_any_selection = cursor_state.selections.iter().any(|s| s.start != s.end);

        if has_any_selection && !has_multi {
            self.delete_selection();
            return;
        }

        let mut full_lsp_change = None::<LspContentChange>;
        let mut lsp_change = None::<LspContentChange>;
        if let Some(lsp) = self.lsp.session.as_ref() {
            if has_multi {
                let old_char_count = self.state_manager.editor().char_count();
                full_lsp_change = Some(lsp.full_document_change(
                    self.state_manager.editor().line_index(),
                    old_char_count,
                    "",
                ));
            } else {
                let offset = self.cursor_offset();
                if offset > 0 {
                    lsp_change = Some(lsp.content_change_for_offsets(
                        self.state_manager.editor().line_index(),
                        offset - 1,
                        offset,
                        "",
                    ));
                }
            }
        }

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(EditCommand::Backspace)) {
            return;
        }
        let after_text = self.state_manager.editor().get_text();
        if after_text == before_text {
            return;
        }
        self.config.text.set(after_text.clone());

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();

        if let Some(change) = lsp_change {
            self.lsp_did_change(change);
        } else if let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }
    }

    pub(super) fn delete_forward(&mut self) {
        let has_multi = !self
            .state_manager
            .editor()
            .secondary_selections()
            .is_empty();

        let cursor_state = self.state_manager.get_cursor_state();
        let has_any_selection = cursor_state.selections.iter().any(|s| s.start != s.end);

        if has_any_selection && !has_multi {
            self.delete_selection();
            return;
        }

        let mut full_lsp_change = None::<LspContentChange>;
        let mut lsp_change = None::<LspContentChange>;
        if let Some(lsp) = self.lsp.session.as_ref() {
            if has_multi {
                let old_char_count = self.state_manager.editor().char_count();
                full_lsp_change = Some(lsp.full_document_change(
                    self.state_manager.editor().line_index(),
                    old_char_count,
                    "",
                ));
            } else {
                let offset = self.cursor_offset();
                let max_offset = self.state_manager.editor().char_count();
                if offset < max_offset {
                    lsp_change = Some(lsp.content_change_for_offsets(
                        self.state_manager.editor().line_index(),
                        offset,
                        offset + 1,
                        "",
                    ));
                }
            }
        }

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(EditCommand::DeleteForward)) {
            return;
        }
        let after_text = self.state_manager.editor().get_text();
        if after_text == before_text {
            return;
        }
        self.config.text.set(after_text.clone());

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();

        if let Some(change) = lsp_change {
            self.lsp_did_change(change);
        } else if let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }
    }

    pub(super) fn delete_selection(&mut self) {
        let has_multi = !self
            .state_manager
            .editor()
            .secondary_selections()
            .is_empty();

        let cursor_state = self.state_manager.get_cursor_state();
        let primary = cursor_state.primary_selection_index;
        let Some(selection) = cursor_state.selections.get(primary) else {
            return;
        };
        let (start, end) = self.selection_offsets(selection);

        if start == end && !has_multi {
            let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
            return;
        }

        let mut full_lsp_change = None::<LspContentChange>;
        let mut lsp_change = None::<LspContentChange>;
        if let Some(lsp) = self.lsp.session.as_ref() {
            if has_multi {
                let old_char_count = self.state_manager.editor().char_count();
                full_lsp_change = Some(lsp.full_document_change(
                    self.state_manager.editor().line_index(),
                    old_char_count,
                    "",
                ));
            } else {
                lsp_change = Some(lsp.content_change_for_offsets(
                    self.state_manager.editor().line_index(),
                    start,
                    end,
                    "",
                ));
            }
        }

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(EditCommand::Backspace)) {
            return;
        }
        let after_text = self.state_manager.editor().get_text();
        if after_text == before_text {
            return;
        }
        self.config.text.set(after_text.clone());

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();

        if let Some(change) = lsp_change {
            self.lsp_did_change(change);
        } else if let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }
    }

    pub(super) fn copy_selection(&mut self) {
        let cursor_state = self.state_manager.get_cursor_state();
        let selections: Vec<Selection> = cursor_state
            .selections
            .into_iter()
            .filter(|s| s.start != s.end)
            .collect();
        if selections.is_empty() {
            return;
        }

        let mut parts = Vec::<String>::new();
        for sel in selections {
            let (start, end) = self.selection_offsets(&sel);
            let len = end.saturating_sub(start);
            if len == 0 {
                continue;
            }
            parts.push(self.state_manager.editor().text_range(start, len));
        }

        self.config.clipboard.set(parts.join("\n"));
    }

    pub(super) fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
    }

    pub(super) fn paste_clipboard(&mut self) {
        let text = self.config.clipboard.get();
        if text.is_empty() {
            return;
        }
        self.insert_text(&text);
    }

    pub(super) fn configure_tab_key_behavior(&mut self) {
        let tab_width = self.config.indent.tab_width.get().max(1);
        let insert_spaces = self.config.indent.insert_spaces.get();

        let _ = self.execute(Command::View(ViewCommand::SetTabWidth { width: tab_width }));
        let _ = self.execute(Command::View(ViewCommand::SetTabKeyBehavior {
            behavior: if insert_spaces {
                TabKeyBehavior::Spaces
            } else {
                TabKeyBehavior::Tab
            },
        }));
    }

    pub(super) fn execute_full_document_edit_and_sync(
        &mut self,
        command: EditCommand,
        insert_like: bool,
    ) -> bool {
        let full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
            let old_char_count = self.state_manager.editor().char_count();
            lsp.full_document_change(self.state_manager.editor().line_index(), old_char_count, "")
        });

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(command)) {
            return false;
        }
        let after_text = self.state_manager.editor().get_text();
        let changed = after_text != before_text;
        if changed {
            self.config.text.set(after_text.clone());
            self.last_insert_time = insert_like.then(Instant::now);
            self.maybe_apply_syntax_highlighting();
        } else if !insert_like {
            self.last_insert_time = None;
        }

        if insert_like {
            self.hide_hover_popup_only();
        } else {
            self.hide_popups();
        }

        if changed && let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }

        true
    }

    pub(super) fn indent_or_tab(&mut self) {
        self.configure_tab_key_behavior();

        // LSP sync: use full-document change since InsertTab can expand to a variable number of
        // spaces (and also applies to multi-cursor / rectangular selections).
        let full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
            let old_char_count = self.state_manager.editor().char_count();
            lsp.full_document_change(self.state_manager.editor().line_index(), old_char_count, "")
        });

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(EditCommand::InsertTab)) {
            return;
        }
        let after_text = self.state_manager.editor().get_text();
        if after_text == before_text {
            return;
        }
        self.config.text.set(after_text.clone());

        self.last_insert_time = Some(Instant::now());
        self.maybe_apply_syntax_highlighting();
        self.hide_hover_popup_only();

        if let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }
    }

    pub(super) fn select_all(&mut self) {
        let editor = self.state_manager.editor();
        let last_line = editor.line_index().line_count().saturating_sub(1);
        let last_col = editor
            .line_index()
            .get_line_text(last_line)
            .map(|s| s.chars().count())
            .unwrap_or(0);

        let _ = self.execute(Command::Cursor(CursorCommand::SetSelection {
            start: Position::new(0, 0),
            end: Position::new(last_line, last_col),
        }));
    }
}
