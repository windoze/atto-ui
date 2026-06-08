// Editor keymap action dispatch for `EditorView`.

use super::*;

impl EditorView {
    pub fn jump_to_position(&mut self, line: usize, column: usize) -> bool {
        self.execute_cursor_action(CursorCommand::MoveTo { line, column }, true)
    }

    pub fn jump_to_offset(&mut self, offset: usize) -> bool {
        let editor = self.state_manager.editor();
        let target = offset.min(editor.char_count());
        let (line, column) = editor.line_index().char_offset_to_position(target);
        self.jump_to_position(line, column)
    }

    pub fn jump_to_utf16_position(&mut self, line: u32, character: u32) -> bool {
        let mut line = usize::try_from(line).unwrap_or(usize::MAX);
        let utf16_column = usize::try_from(character).unwrap_or(usize::MAX);
        let line_index = self.state_manager.editor().line_index();
        line = line.min(line_index.line_count().saturating_sub(1));
        let line_text = line_index.get_line_text(line).unwrap_or_default();
        let column =
            editor_core_lsp::LspCoordinateConverter::utf16_to_char_offset(&line_text, utf16_column);
        self.jump_to_position(line, column)
    }

    fn execute_cursor_action(&mut self, command: CursorCommand, clear_selection: bool) -> bool {
        self.hide_popups();
        if !self.execute(Command::Cursor(command)) {
            return false;
        }
        if clear_selection {
            let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
            self.rect_selection_anchor = None;
        }
        self.adjust_scroll();
        true
    }

    fn jump_to_diagnostic(&mut self, direction: DiagnosticJumpDirection) -> bool {
        let editor = self.state_manager.editor();
        let mut diagnostics = editor.diagnostics().iter().enumerate().collect::<Vec<_>>();
        if diagnostics.is_empty() {
            return false;
        }

        diagnostics
            .sort_by_key(|(_idx, diagnostic)| (diagnostic.range.start, diagnostic.range.end));

        let current = self.cursor_offset();
        let selected = match direction {
            DiagnosticJumpDirection::Next => diagnostics
                .iter()
                .position(|(_idx, diagnostic)| diagnostic.range.start > current)
                .unwrap_or(0),
            DiagnosticJumpDirection::Prev => diagnostics
                .iter()
                .rposition(|(_idx, diagnostic)| diagnostic.range.start < current)
                .unwrap_or_else(|| diagnostics.len().saturating_sub(1)),
        };

        let (diagnostic_idx, diagnostic) = diagnostics[selected];
        let target_offset = diagnostic.range.start.min(editor.char_count());
        let (line, column) = editor.line_index().char_offset_to_position(target_offset);

        self.hide_popups();
        let moved = self.execute(Command::Cursor(CursorCommand::MoveTo { line, column }));
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
        if moved {
            self.lsp.diagnostic_cursor = Some(diagnostic_idx);
            self.adjust_scroll();
        }
        moved
    }

    /// Executes an editor command action without synthesizing a key event.
    pub fn handle_editor_action(&mut self, action: EditorAction) -> bool {
        self.handle_action(action)
    }

    pub(super) fn handle_action(&mut self, action: EditorAction) -> bool {
        if self.config.read_only.get() && action_mutates_document(action) {
            self.hide_popups();
            return false;
        }

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
            EditorAction::Indent => {
                self.configure_tab_key_behavior();
                self.execute_full_document_edit_and_sync(EditCommand::Indent, false)
            }
            EditorAction::Outdent => {
                self.configure_tab_key_behavior();
                self.execute_full_document_edit_and_sync(EditCommand::Outdent, false)
            }
            EditorAction::SplitLine => {
                self.execute_full_document_edit_and_sync(EditCommand::SplitLine, false)
            }
            EditorAction::ToggleComment => {
                let Some(config) = self.config.comment.get() else {
                    self.hide_popups();
                    return true;
                };
                if !config.has_line() && !config.has_block() {
                    self.hide_popups();
                    return true;
                }
                self.execute_full_document_edit_and_sync(
                    EditCommand::ToggleComment { config },
                    false,
                )
            }
            EditorAction::JoinLines => {
                self.execute_full_document_edit_and_sync(EditCommand::JoinLines, false)
            }
            EditorAction::MoveLinesUp => {
                self.execute_full_document_edit_and_sync(EditCommand::MoveLinesUp, false)
            }
            EditorAction::MoveLinesDown => {
                self.execute_full_document_edit_and_sync(EditCommand::MoveLinesDown, false)
            }
            EditorAction::DuplicateLines => {
                self.execute_full_document_edit_and_sync(EditCommand::DuplicateLines, false)
            }
            EditorAction::DeleteLines => {
                self.execute_full_document_edit_and_sync(EditCommand::DeleteLines, false)
            }

            EditorAction::MoveLeft => {
                self.move_cursor(0, -1, false);
                true
            }
            EditorAction::MoveRight => {
                self.move_cursor(0, 1, false);
                true
            }
            EditorAction::MoveWordLeft => {
                self.execute_cursor_action(CursorCommand::MoveWordLeft, true)
            }
            EditorAction::MoveWordRight => {
                self.execute_cursor_action(CursorCommand::MoveWordRight, true)
            }
            EditorAction::MoveToMatchingBracket => {
                self.execute_cursor_action(CursorCommand::MoveToMatchingBracket, true)
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
            EditorAction::AddCursorAbove => {
                self.execute_cursor_action(CursorCommand::AddCursorAbove, false)
            }
            EditorAction::AddCursorBelow => {
                self.execute_cursor_action(CursorCommand::AddCursorBelow, false)
            }
            EditorAction::AddNextOccurrence => self.execute_cursor_action(
                CursorCommand::AddNextOccurrence {
                    options: SearchOptions::default(),
                },
                false,
            ),
            EditorAction::AddAllOccurrences => self.execute_cursor_action(
                CursorCommand::AddAllOccurrences {
                    options: SearchOptions::default(),
                },
                false,
            ),
            EditorAction::ExpandSelection => {
                self.execute_cursor_action(CursorCommand::ExpandSelection, false)
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
                if self.rename_popup.get().is_some() || self.lsp.pending_prepare_rename.is_some() {
                    self.rename_popup.set(None);
                    self.lsp.pending_prepare_rename = None;
                    self.lsp.pending_rename = None;
                    self.lsp.rename_target = None;
                    return true;
                }
                if self.code_action_popup.get().is_some() {
                    self.code_action_popup.set(None);
                    self.lsp.code_action_items.clear();
                    return true;
                }
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
            EditorAction::LspCodeAction => {
                self.request_code_action_now();
                true
            }
            EditorAction::LspRename => {
                self.request_prepare_rename_now();
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
            EditorAction::LspNextDiagnostic => {
                self.jump_to_diagnostic(DiagnosticJumpDirection::Next)
            }
            EditorAction::LspPrevDiagnostic => {
                self.jump_to_diagnostic(DiagnosticJumpDirection::Prev)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiagnosticJumpDirection {
    Next,
    Prev,
}

fn action_mutates_document(action: EditorAction) -> bool {
    matches!(
        action,
        EditorAction::Undo
            | EditorAction::Redo
            | EditorAction::Cut
            | EditorAction::Paste
            | EditorAction::Replace
            | EditorAction::Backspace
            | EditorAction::DeleteForward
            | EditorAction::InsertNewline
            | EditorAction::InsertTab
            | EditorAction::Indent
            | EditorAction::Outdent
            | EditorAction::SplitLine
            | EditorAction::ToggleComment
            | EditorAction::JoinLines
            | EditorAction::MoveLinesUp
            | EditorAction::MoveLinesDown
            | EditorAction::DuplicateLines
            | EditorAction::DeleteLines
            | EditorAction::LspCodeAction
            | EditorAction::LspRename
    )
}
