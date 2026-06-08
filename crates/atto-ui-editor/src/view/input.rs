// Keyboard/paste input handling for `EditorView`.

use super::*;

impl EditorView {
    pub(super) fn handle_key_event(&mut self, key: KeyEvent) -> EventResult {
        let Some(chord) = KeyChord::from_key_event(key) else {
            return EventResult::ignored();
        };

        if self.search_is_active() {
            return self.handle_search_key_event(key);
        }

        if self.rename_popup.get().is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.cancel_rename_popup();
                    return EventResult::consumed();
                }
                KeyCode::Enter => {
                    self.submit_rename_popup();
                    return EventResult::consumed();
                }
                KeyCode::Backspace => {
                    self.backspace_rename_popup();
                    return EventResult::consumed();
                }
                KeyCode::Delete => {
                    self.delete_rename_popup();
                    return EventResult::consumed();
                }
                KeyCode::Left => {
                    self.move_rename_popup_cursor(-1);
                    return EventResult::consumed();
                }
                KeyCode::Right => {
                    self.move_rename_popup_cursor(1);
                    return EventResult::consumed();
                }
                KeyCode::Home => {
                    self.move_rename_popup_cursor_to(0);
                    return EventResult::consumed();
                }
                KeyCode::End => {
                    if let Some(model) = self.rename_popup.get() {
                        self.move_rename_popup_cursor_to(model.value.chars().count());
                    }
                    return EventResult::consumed();
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    self.insert_rename_popup_char(ch);
                    return EventResult::consumed();
                }
                _ => return EventResult::consumed(),
            }
        }

        if let Some(since) = self.pending_ctrl_k.take()
            && since.elapsed() <= atto_ui::app::DEFAULT_KEY_SEQUENCE_TIMEOUT
            && is_ctrl_char_chord(chord, 'f')
        {
            let requested = self.request_format_document_now(false);
            return if requested {
                EventResult::consumed()
            } else {
                EventResult::ignored()
            };
        }

        if is_ctrl_char_chord(chord, 'k') {
            self.clear_secondary_selections();
            self.pending_ctrl_k = Some(Instant::now());
            return EventResult::consumed();
        }

        // Code action popup keyboard navigation/accept.
        if let Some(popup) = self.code_action_popup.get() {
            match key.code {
                KeyCode::Esc => {
                    self.code_action_popup.set(None);
                    self.lsp.code_action_items.clear();
                    return EventResult::consumed();
                }
                KeyCode::Enter => {
                    if popup.selected < popup.items.len() {
                        let mut popup = popup;
                        popup.accept = Some(popup.selected);
                        self.code_action_popup.set(Some(popup));
                        self.process_code_action_accept();
                    } else {
                        self.code_action_popup.set(None);
                        self.lsp.code_action_items.clear();
                    }
                    return EventResult::consumed();
                }
                KeyCode::Up => {
                    self.select_code_action_relative(-1);
                    return EventResult::consumed();
                }
                KeyCode::Down => {
                    self.select_code_action_relative(1);
                    return EventResult::consumed();
                }
                KeyCode::PageUp => {
                    self.select_code_action_relative(-5);
                    return EventResult::consumed();
                }
                KeyCode::PageDown => {
                    self.select_code_action_relative(5);
                    return EventResult::consumed();
                }
                _ => {
                    self.code_action_popup.set(None);
                    self.lsp.code_action_items.clear();
                }
            }
        }

        // Completion popup keyboard navigation/accept (editor keeps focus, popup stays non-modal).
        if let Some(popup) = self.completion_popup.get() {
            match key.code {
                KeyCode::Esc => {
                    self.completion_popup.set(None);
                    return EventResult::consumed();
                }
                KeyCode::Enter => {
                    if popup.selected < popup.items.len() {
                        let mut popup = popup;
                        popup.accept = Some(popup.selected);
                        self.completion_popup.set(Some(popup));
                        self.process_completion_accept();
                    } else {
                        self.completion_popup.set(None);
                    }
                    return EventResult::consumed();
                }
                KeyCode::Up => {
                    self.select_completion_relative(-1);
                    return EventResult::consumed();
                }
                KeyCode::Down => {
                    self.select_completion_relative(1);
                    return EventResult::consumed();
                }
                KeyCode::PageUp => {
                    self.select_completion_relative(-5);
                    return EventResult::consumed();
                }
                KeyCode::PageDown => {
                    self.select_completion_relative(5);
                    return EventResult::consumed();
                }
                _ => {
                    // Any other key dismisses completion; the key is then handled normally.
                    self.completion_popup.set(None);
                }
            }
        }

        let keymap: EditorKeymap = self.config.keymap.get();
        if let Some(action) = keymap.get(chord) {
            let consumed = self.handle_action(action);
            return if consumed {
                EventResult::consumed()
            } else {
                EventResult::ignored()
            };
        }

        // Default text insertion: Char(c) without Ctrl/Alt.
        match key.code {
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                if self.config.read_only.get() {
                    return EventResult::ignored();
                }
                self.insert_text(&c.to_string());
                self.adjust_scroll();
                if matches!(c, '(' | ',') {
                    self.request_signature_help_now();
                } else {
                    self.clear_signature_help_popup();
                }
                return EventResult::consumed();
            }
            _ => {}
        }

        EventResult::ignored()
    }

    fn select_code_action_relative(&mut self, delta: isize) {
        let Some(mut popup) = self.code_action_popup.get() else {
            return;
        };
        if popup.items.is_empty() {
            return;
        }
        let len = popup.items.len() as isize;
        let mut selected = popup.selected as isize + delta;
        if selected < 0 {
            selected = 0;
        }
        if selected >= len {
            selected = len - 1;
        }
        popup.selected = selected as usize;

        let visible = popup.rect.height.saturating_sub(2) as usize;
        if visible > 0 {
            if popup.selected < popup.scroll {
                popup.scroll = popup.selected;
            } else if popup.selected >= popup.scroll + visible {
                popup.scroll = popup.selected.saturating_sub(visible.saturating_sub(1));
            }
        }

        self.code_action_popup.set(Some(popup));
    }

    fn select_completion_relative(&mut self, delta: isize) {
        let Some(mut popup) = self.completion_popup.get() else {
            return;
        };
        if popup.items.is_empty() {
            return;
        }
        let len = popup.items.len() as isize;
        let mut selected = popup.selected as isize + delta;
        if selected < 0 {
            selected = 0;
        }
        if selected >= len {
            selected = len - 1;
        }
        popup.selected = selected as usize;

        // Ensure selected is within visible window.
        let visible = popup.rect.height.saturating_sub(2) as usize;
        if visible > 0 {
            if popup.selected < popup.scroll {
                popup.scroll = popup.selected;
            } else if popup.selected >= popup.scroll + visible {
                popup.scroll = popup.selected.saturating_sub(visible.saturating_sub(1));
            }
        }

        self.completion_popup.set(Some(popup));
    }
}

fn is_ctrl_char_chord(chord: KeyChord, ch: char) -> bool {
    chord.code == KeyCode::Char(ch) && chord.modifiers == KeyModifiers::CONTROL
}
