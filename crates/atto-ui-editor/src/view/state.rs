// Shared state synchronization and small cross-module helpers for `EditorView`.

use super::*;

impl EditorView {
    pub(super) fn active_cursor_position(&self) -> Position {
        self.state_manager.get_cursor_state().position
    }

    pub(super) fn editor_theme(&self) -> EditorTheme {
        let theme_set = self.theme.get();
        let language_id = self.config.language_id.get();
        theme_set.for_language(language_id.as_str()).clone()
    }

    pub(super) fn sync_external_text_if_dirty(&mut self) {
        if !self.config.text.check_dirty(&mut self.text_observer) {
            return;
        }

        let external = self.config.text.get();
        let current = self.state_manager.editor().get_text();
        if external == current {
            return;
        }

        let old_char_count = self.state_manager.editor().char_count();
        let _ = self
            .state_manager
            .execute(Command::Edit(EditCommand::Replace {
                start: 0,
                length: old_char_count,
                text: external,
            }));

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();
    }

    pub(super) fn hide_popups(&mut self) {
        if self.hover_popup.get().is_some() {
            self.hover_popup.set(None);
        }
        if self.completion_popup.get().is_some() {
            self.completion_popup.set(None);
        }
        if self.code_action_popup.get().is_some() {
            self.code_action_popup.set(None);
        }
        self.lsp.hover_due = None;
        self.lsp.hover_pending_request = None;
        self.lsp.hover_anchor = None;
        self.lsp.hover_target = None;
        self.lsp.hover_requested = None;
        self.lsp.hover_suppressed_position = None;
        self.hover_popup_dismissed.set(None);
        self.lsp.completion_pending_request = None;
        self.lsp.completion_requested_position = None;
        self.lsp.pending_goto = None;
        self.lsp.pending_code_action = None;
        self.lsp.code_action_items.clear();
    }

    pub(super) fn selection_offsets(&self, selection: &Selection) -> (usize, usize) {
        let start_offset = self
            .state_manager
            .editor()
            .line_index()
            .position_to_char_offset(selection.start.line, selection.start.column);
        let end_offset = self
            .state_manager
            .editor()
            .line_index()
            .position_to_char_offset(selection.end.line, selection.end.column);
        (start_offset.min(end_offset), start_offset.max(end_offset))
    }

    pub(super) fn cursor_offset(&self) -> usize {
        let pos = self.active_cursor_position();
        self.state_manager
            .editor()
            .line_index()
            .position_to_char_offset(pos.line, pos.column)
    }

    pub(super) fn execute(&mut self, command: Command) -> bool {
        self.state_manager.execute(command).is_ok()
    }

    pub(super) fn execute_and_sync_text(&mut self, command: Command) -> bool {
        let before = self.state_manager.editor().get_text();
        if !self.execute(command) {
            return false;
        }
        let after = self.state_manager.editor().get_text();
        if after != before {
            self.config.text.set(after);
        }
        true
    }
}
