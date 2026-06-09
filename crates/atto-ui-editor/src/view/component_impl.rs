// atto-ui component trait implementations for `EditorView`.

use super::*;

impl ::atto_ui::composable::Component for EditorView {
    fn property_names(&self) -> Vec<&'static str> {
        vec![
            "text",
            "language_id",
            "show_line_numbers",
            "show_folding_markers",
            "read_only",
            "tab_width",
            "insert_spaces",
            "format_on_save",
            "trim_trailing_whitespace_on_save",
            "inlay_hints_enabled",
        ]
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        match name {
            "text" => Some(ComponentValue::String(self.config.text.get())),
            "language_id" => Some(ComponentValue::String(self.config.language_id.get())),
            "show_line_numbers" => Some(ComponentValue::Bool(self.config.show_line_numbers.get())),
            "show_folding_markers" => {
                Some(ComponentValue::Bool(self.config.show_folding_markers.get()))
            }
            "read_only" => Some(ComponentValue::Bool(self.config.read_only.get())),
            "tab_width" => Some(ComponentValue::U64(
                self.config.indent.tab_width.get() as u64
            )),
            "insert_spaces" => Some(ComponentValue::Bool(self.config.indent.insert_spaces.get())),
            "format_on_save" => Some(ComponentValue::Bool(self.config.format_on_save.get())),
            "trim_trailing_whitespace_on_save" => Some(ComponentValue::Bool(
                self.config.trim_trailing_whitespace_on_save.get(),
            )),
            "inlay_hints_enabled" => {
                Some(ComponentValue::Bool(self.config.inlay_hints.enabled.get()))
            }
            _ => None,
        }
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        match name {
            "text" => {
                let v = <String as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.text.set(v);
                Ok(())
            }
            "language_id" => {
                let v = <String as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.language_id.set(v);
                Ok(())
            }
            "show_line_numbers" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.show_line_numbers.set(v);
                Ok(())
            }
            "show_folding_markers" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.show_folding_markers.set(v);
                Ok(())
            }
            "read_only" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.read_only.set(v);
                Ok(())
            }
            "tab_width" => {
                let v = <usize as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.indent.tab_width.set(v);
                Ok(())
            }
            "insert_spaces" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.indent.insert_spaces.set(v);
                Ok(())
            }
            "format_on_save" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.format_on_save.set(v);
                Ok(())
            }
            "trim_trailing_whitespace_on_save" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.trim_trailing_whitespace_on_save.set(v);
                Ok(())
            }
            "inlay_hints_enabled" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.inlay_hints.enabled.set(v);
                if !v {
                    self.reset_inlay_hint_tracking();
                    self.clear_lsp_inlay_hints();
                }
                Ok(())
            }
            _ => Err(ComponentError::unsupported_property(name)),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        // Sync external text changes and config at draw time too (tick-driven apps).
        self.sync_external_text_if_dirty();
        if self.config.syntax.check_dirty(&mut self.syntax_observer) {
            self.configure_syntax_processor();
        }
        self.maybe_start_or_stop_lsp();

        // Hover popup can be dismissed from its own tooltip window; reflect that here.
        self.consume_hover_popup_dismissed();

        // Poll LSP + hover timers.
        self.maybe_poll_lsp();
        self.maybe_end_undo_group_after_idle();

        if ctx.is_focused {
            if !self.focused_last_frame {
                self.schedule_hover_after_delay();
            }
            self.maybe_fire_hover();
        } else {
            self.hide_popups();
        }
        self.focused_last_frame = ctx.is_focused;

        // Handle completion accept requested by mouse events on the completion popup window.
        self.process_completion_accept();
        self.process_code_action_accept();

        self.render(frame, area, ctx);
    }
}

impl ::atto_ui::composable::DragAndDrop for EditorView {}

impl ::atto_ui::composable::Layout for EditorView {
    fn min_width(&self) -> u16 {
        8
    }

    fn min_height(&self) -> u16 {
        3
    }
}

impl ::atto_ui::composable::Scrollable for EditorView {
    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        (0, (scroll_top.min(u16::MAX as usize)) as u16)
    }

    fn set_scroll_offset(&mut self, _x: u16, y: u16) {
        let viewport_height = self.state_manager.get_viewport_state().height.unwrap_or(0);
        if viewport_height == 0 {
            return;
        }
        let desired = y as usize;
        let max = self.max_scroll_top(viewport_height);
        self.state_manager.set_scroll_top(desired.min(max));
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.config.scroll.config.get()
    }
}

impl ::atto_ui::composable::FocusNav for EditorView {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl ::atto_ui::composable::DynamicTree for EditorView {}

impl ::atto_ui::composable::EventHandling for EditorView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        // Keep internal state in sync with external bindings (without constantly cloning).
        self.sync_external_text_if_dirty();

        // Runtime config changes.
        if self.config.syntax.check_dirty(&mut self.syntax_observer) {
            self.configure_syntax_processor();
        }
        self.maybe_start_or_stop_lsp();

        // Popups should be dismissed whenever focus is lost.
        if !ctx.is_focused {
            self.hide_popups();
            return EventResult::ignored();
        }

        // Hover popup can be dismissed from its own tooltip window; reflect that here.
        self.consume_hover_popup_dismissed();

        // Keyboard input and clicks should dismiss hover immediately, but mouse movement should
        // allow the hover tooltip to track the pointer. Esc is special-cased so the popup close
        // path can set suppression state.
        let preserve_hover = matches!(
            event,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                ..
            })
        ) || matches!(
            event,
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                ..
            })
        );
        if !preserve_hover {
            self.hide_hover_popup_only();
        }

        // Apply completion accept queued by mouse (from tooltip popup window).
        self.process_completion_accept();
        self.process_code_action_accept();

        let res = match event {
            Event::Paste(text) => {
                if self.config.read_only.get() {
                    return EventResult::ignored();
                }
                self.insert_text(text);
                self.clear_signature_help_popup();
                self.adjust_scroll();
                EventResult::consumed()
            }
            Event::Key(key) => self.handle_key_event(*key),
            Event::Mouse(m) => self.handle_mouse(*m, ctx.mouse_coordinate_space),
            _ => EventResult::ignored(),
        };

        // Schedule hover for when the user goes idle again.
        self.schedule_hover_after_delay();

        res
    }
}
