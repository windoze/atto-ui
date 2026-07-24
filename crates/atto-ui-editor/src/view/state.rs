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
        if self.signature_help_popup.get().is_some() {
            self.signature_help_popup.set(None);
        }
        if self.code_action_popup.get().is_some() {
            self.code_action_popup.set(None);
        }
        if self.rename_popup.get().is_some() {
            self.rename_popup.set(None);
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
        self.lsp.pending_signature_help = None;
        self.lsp.signature_help_requested_position = None;
        self.lsp.pending_goto = None;
        self.lsp.pending_code_action = None;
        self.lsp.code_action_items.clear();
        self.lsp.pending_prepare_rename = None;
        self.lsp.pending_rename = None;
        self.lsp.rename_target = None;
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

    /// Execute a document-mutating command and forward the *actual* edit to the LSP server via the
    /// structured `TextDelta` the edit produced.
    ///
    /// Returns whether the document changed. The caller owns every other side effect (syntax,
    /// popups, scroll, `last_insert_time`) — this only executes the command, syncs `config.text`,
    /// and drives `didChange`.
    ///
    /// Why delta-based: hand-computing an incremental LSP range (`content_change_for_offsets`)
    /// assumes how many chars an edit touched, which is wrong for auto-pair deletion (removes two
    /// chars), re-indentation, and multi-cursor edits. `take_last_text_delta()` returns exactly
    /// what changed, so `LspSession::did_change_from_text_delta` stays correct for all of them.
    ///
    /// Invariant: `last_text_delta` is overwritten (not accumulated) by each `execute`, so this
    /// MUST run once per document edit and always take the delta — the upstream LSP mirror asserts
    /// `delta.before_char_count == mirror_char_count`, so a skipped or doubled edit desyncs it.
    /// Using the delta (not a `get_text()` diff) to decide `changed` keeps that mirror advancing
    /// exactly in lockstep with core.
    pub(super) fn execute_edit_and_sync_delta(&mut self, command: Command) -> bool {
        if !self.execute(command) {
            return false;
        }
        // Take unconditionally so a no-op edit can't leave a stale delta behind to be mis-sent on
        // the next edit; only a non-empty delta represents a real change.
        let delta = self.state_manager.take_last_text_delta();
        let changed = delta.as_ref().is_some_and(|d| !d.is_empty());
        if changed {
            self.config.text.set(self.state_manager.editor().get_text());
            if let Some(delta) = delta {
                self.lsp_did_change_from_delta(&delta);
            }
        }
        changed
    }
}
