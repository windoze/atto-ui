// Text editing, clipboard, and indentation operations for `EditorView`.

use super::*;

impl EditorView {
    pub(super) fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        if !self.execute_edit_and_sync_delta(Command::Edit(EditCommand::InsertText {
            text: text.to_string(),
        })) {
            return;
        }

        self.last_insert_time = Some(Instant::now());
        self.maybe_apply_syntax_highlighting();
        self.hide_hover_popup_only();
        self.clear_signature_help_popup();
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

        if !self.execute_edit_and_sync_delta(Command::Edit(EditCommand::Backspace)) {
            return;
        }

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();
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

        if !self.execute_edit_and_sync_delta(Command::Edit(EditCommand::DeleteForward)) {
            return;
        }

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();
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

        if !self.execute_edit_and_sync_delta(Command::Edit(EditCommand::Backspace)) {
            return;
        }

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();
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

    pub(super) fn configure_typing_behavior(&mut self) {
        self.configure_tab_key_behavior();
        let _ = self.execute(Command::View(ViewCommand::SetIndentationConfig {
            config: self.config.indent.language.get(),
        }));
        let _ = self.execute(Command::View(ViewCommand::SetAutoPairsConfig {
            config: self.config.auto_pairs.get(),
        }));
    }

    pub(super) fn execute_full_document_edit_and_sync(
        &mut self,
        command: EditCommand,
        insert_like: bool,
    ) -> bool {
        // The delta helper handles both incremental and expanding edits correctly, so there is no
        // longer a "full-document" special case — the name is kept for its existing callers.
        let changed = self.execute_edit_and_sync_delta(Command::Edit(command));
        if changed {
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

        true
    }

    pub(super) fn indent_or_tab(&mut self) {
        self.configure_tab_key_behavior();

        // InsertTab can expand to a variable number of spaces and also applies to multi-cursor /
        // rectangular selections; the delta helper reports exactly what changed, so no assumption
        // about the touched range is needed.
        if !self.execute_edit_and_sync_delta(Command::Edit(EditCommand::InsertTab)) {
            return;
        }

        self.last_insert_time = Some(Instant::now());
        self.maybe_apply_syntax_highlighting();
        self.hide_hover_popup_only();
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
