// Editor keymap action dispatch for `EditorView`.

use super::*;

impl EditorView {
    pub(super) fn handle_action(&mut self, action: EditorAction) -> bool {
        match action {
            EditorAction::Undo => {
                self.hide_popups();
                let old_char_count = self.state_manager.editor().char_count();
                let full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
                    lsp.full_document_change(
                        self.state_manager.editor().line_index(),
                        old_char_count,
                        "",
                    )
                });
                let before = self.state_manager.editor().get_text();
                if !self.execute(Command::Edit(EditCommand::Undo)) {
                    return false;
                }
                let after = self.state_manager.editor().get_text();
                if after != before {
                    self.config.text.set(after.clone());
                }
                self.maybe_apply_syntax_highlighting();
                if let Some(mut change) = full_lsp_change {
                    change.text = after;
                    self.lsp_did_change(change);
                }
                true
            }
            EditorAction::Redo => {
                self.hide_popups();
                let old_char_count = self.state_manager.editor().char_count();
                let full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
                    lsp.full_document_change(
                        self.state_manager.editor().line_index(),
                        old_char_count,
                        "",
                    )
                });
                let before = self.state_manager.editor().get_text();
                if !self.execute(Command::Edit(EditCommand::Redo)) {
                    return false;
                }
                let after = self.state_manager.editor().get_text();
                if after != before {
                    self.config.text.set(after.clone());
                }
                self.maybe_apply_syntax_highlighting();
                if let Some(mut change) = full_lsp_change {
                    change.text = after;
                    self.lsp_did_change(change);
                }
                true
            }
            EditorAction::Copy => {
                self.copy_selection();
                true
            }
            EditorAction::Cut => {
                self.cut_selection();
                true
            }
            EditorAction::Paste => {
                self.paste_clipboard();
                true
            }
            EditorAction::SelectAll => {
                self.select_all();
                true
            }

            EditorAction::Find => {
                self.hide_popups();
                let seed = self.search_seed_from_selection();
                self.open_find(seed.as_deref());
                true
            }
            EditorAction::Replace => {
                self.hide_popups();
                let seed = self.search_seed_from_selection();
                self.open_replace(seed.as_deref());
                true
            }
            EditorAction::FindNext => {
                self.hide_popups();
                if self.search_query_is_empty() {
                    self.open_find(None);
                } else {
                    self.search_find_next();
                }
                true
            }
            EditorAction::FindPrev => {
                self.hide_popups();
                if self.search_query_is_empty() {
                    self.open_find(None);
                } else {
                    self.search_find_prev();
                }
                true
            }

            EditorAction::Backspace => {
                self.backspace();
                true
            }
            EditorAction::DeleteForward => {
                self.delete_forward();
                true
            }
            EditorAction::InsertNewline => {
                self.insert_text("\n");
                true
            }
            EditorAction::InsertTab => {
                self.indent_or_tab();
                true
            }

            EditorAction::MoveLeft => {
                self.move_cursor(0, -1, false);
                true
            }
            EditorAction::MoveRight => {
                self.move_cursor(0, 1, false);
                true
            }
            EditorAction::MoveUp => {
                self.move_cursor(-1, 0, false);
                true
            }
            EditorAction::MoveDown => {
                self.move_cursor(1, 0, false);
                true
            }
            EditorAction::MoveHome => {
                self.move_home_end(false, false);
                true
            }
            EditorAction::MoveEnd => {
                self.move_home_end(true, false);
                true
            }
            EditorAction::PageUp => {
                self.page_scroll(false, false);
                true
            }
            EditorAction::PageDown => {
                self.page_scroll(true, false);
                true
            }

            EditorAction::SelectLeft => {
                self.move_cursor(0, -1, true);
                true
            }
            EditorAction::SelectRight => {
                self.move_cursor(0, 1, true);
                true
            }
            EditorAction::SelectUp => {
                self.move_cursor(-1, 0, true);
                true
            }
            EditorAction::SelectDown => {
                self.move_cursor(1, 0, true);
                true
            }
            EditorAction::SelectHome => {
                self.move_home_end(false, true);
                true
            }
            EditorAction::SelectEnd => {
                self.move_home_end(true, true);
                true
            }
            EditorAction::SelectPageUp => {
                self.page_scroll(false, true);
                true
            }
            EditorAction::SelectPageDown => {
                self.page_scroll(true, true);
                true
            }

            EditorAction::ClearSecondarySelections => {
                self.clear_secondary_selections();
                true
            }
            EditorAction::ToggleRectSelection => {
                self.toggle_rect_selection();
                true
            }

            EditorAction::ToggleFoldAtCursor => {
                self.toggle_fold_at_cursor();
                true
            }
            EditorAction::UnfoldAll => {
                self.unfold_all();
                true
            }

            EditorAction::CancelPopup => {
                if self.completion_popup.get().is_some() {
                    self.completion_popup.set(None);
                    return true;
                }
                if let Some(model) = self.hover_popup.get() {
                    // Treat Esc dismissal as an explicit close: don't re-show at the same hover
                    // position unless the mouse moves.
                    self.lsp.hover_suppressed_position = Some(model.anchor);
                    self.hover_popup.set(None);
                    self.lsp.hover_due = None;
                    self.lsp.hover_pending_request = None;
                    self.lsp.hover_target = None;
                    self.lsp.hover_requested = None;
                    return true;
                }

                // No popups: Esc should clear selection (and multi-cursor) if present.
                let cursor_state = self.state_manager.get_cursor_state();
                let has_primary_selection = cursor_state
                    .selections
                    .get(cursor_state.primary_selection_index)
                    .is_some_and(|s| s.start != s.end);
                let has_secondary = !self
                    .state_manager
                    .editor()
                    .secondary_selections()
                    .is_empty();
                if has_primary_selection || has_secondary {
                    let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
                    let _ = self.execute(Command::Cursor(CursorCommand::ClearSecondarySelections));
                    self.rect_selection_anchor = None;
                    return true;
                }

                false
            }

            EditorAction::LspRequestHover => {
                self.request_hover_now();
                true
            }
            EditorAction::LspRequestCompletion => {
                self.request_completion_now();
                true
            }
            EditorAction::LspGotoDefinition => {
                self.request_goto(EditorLspGotoKind::Definition);
                true
            }
            EditorAction::LspGotoDeclaration => {
                self.request_goto(EditorLspGotoKind::Declaration);
                true
            }
            EditorAction::LspGotoTypeDefinition => {
                self.request_goto(EditorLspGotoKind::TypeDefinition);
                true
            }
            EditorAction::LspGotoImplementation => {
                self.request_goto(EditorLspGotoKind::Implementation);
                true
            }
            EditorAction::LspGotoReferences => {
                self.request_goto(EditorLspGotoKind::References);
                true
            }

            EditorAction::ToggleLineNumbers => {
                let cur = self.config.show_line_numbers.get();
                self.config.show_line_numbers.set(!cur);
                true
            }
            EditorAction::ToggleFoldingMarkers => {
                let cur = self.config.show_folding_markers.get();
                self.config.show_folding_markers.set(!cur);
                true
            }
        }
    }
}
