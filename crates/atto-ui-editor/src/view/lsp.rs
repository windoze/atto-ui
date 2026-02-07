// LSP integration + hover/completion popup plumbing.

use super::*;

impl EditorView {
    pub(super) fn lsp_did_change(&mut self, change: LspContentChange) {
        let result = {
            let Some(lsp) = self.lsp.session.as_mut() else {
                return;
            };
            lsp.did_change(change)
        };

        if result.is_err() {
            self.lsp.session = None;
            editor_core_lsp::clear_lsp_state(&mut self.state_manager);
            self.maybe_apply_syntax_highlighting();
        }
    }

    pub(super) fn maybe_poll_lsp(&mut self) {
        let poll_result = {
            let Some(lsp) = self.lsp.session.as_mut() else {
                return;
            };
            self.state_manager.apply_processor(lsp)
        };

        if poll_result.is_err() {
            self.lsp.session = None;
            editor_core_lsp::clear_lsp_state(&mut self.state_manager);
            self.maybe_apply_syntax_highlighting();
            return;
        }

        // Drain events (hover/completion/goto responses, UX messages, etc.)
        let Some(lsp) = self.lsp.session.as_mut() else {
            return;
        };
        for ev in lsp.drain_events() {
            let editor_core_lsp::LspEvent::Response(resp) = ev else {
                continue;
            };

            let method = resp.method;
            let id = resp.id;
            let result = resp.result;
            let error = resp.error;

            if let Some((pending_id, kind)) = self.lsp.pending_goto
                && pending_id == id
            {
                let locs = result
                    .as_ref()
                    .map(locations_from_value)
                    .unwrap_or_default();
                self.events.push(EditorEvent::LspGoto {
                    kind,
                    locations: locs,
                });
                self.lsp.pending_goto = None;
            }

            if let Some(pending_id) = self.lsp.hover_pending_request
                && pending_id == id
                && method.as_str() == "textDocument/hover"
            {
                self.lsp.hover_pending_request = None;
                let requested = self.lsp.hover_requested.take();
                if error.is_some() {
                    self.hover_popup.set(None);
                    continue;
                }

                let Some(result) = result.as_ref() else {
                    self.hover_popup.set(None);
                    continue;
                };

                let anchor = requested.or(self.lsp.hover_anchor).or_else(|| {
                    self.cursor_screen_position()
                        .and_then(|p| p)
                        .map(|p| HoverAnchor {
                            position: self.active_cursor_position(),
                            screen: p,
                        })
                });

                if let Some(anchor) = anchor {
                    self.handle_lsp_hover_response(result, anchor);
                } else {
                    self.hover_popup.set(None);
                }
            }

            if let Some(pending_id) = self.lsp.completion_pending_request
                && pending_id == id
                && method.as_str() == "textDocument/completion"
            {
                self.lsp.completion_pending_request = None;
                self.lsp.completion_requested_position = None;
                if let Some(result) = result.as_ref() {
                    self.handle_lsp_completion_response(result);
                } else {
                    self.completion_popup.set(None);
                }
            }
        }
    }

    pub(super) fn maybe_start_or_stop_lsp(&mut self) {
        if !self.config.lsp.check_dirty(&mut self.lsp_observer) {
            return;
        }

        match self.config.lsp.get() {
            EditorLspMode::Disabled => {
                self.lsp.session = None;
                editor_core_lsp::clear_lsp_state(&mut self.state_manager);
                self.maybe_apply_syntax_highlighting();
                self.hide_popups();
            }
            EditorLspMode::Enabled(cfg) => {
                // Best-effort restart on changes.
                self.lsp.session = None;
                editor_core_lsp::clear_lsp_state(&mut self.state_manager);
                self.hide_popups();
                self.start_lsp_session(cfg);
            }
        }
    }

    pub(super) fn start_lsp_session(&mut self, cfg: crate::config::EditorLspConfig) {
        if cfg.command.is_empty() {
            return;
        }

        let program = cfg.command[0].clone();
        let args = cfg.command.iter().skip(1).cloned().collect::<Vec<_>>();

        let mut cmd = ProcessCommand::new(&program);
        cmd.args(args);
        cmd.stderr(std::process::Stdio::null());

        let token_types = vec![
            "namespace",
            "type",
            "class",
            "enum",
            "interface",
            "struct",
            "typeParameter",
            "parameter",
            "variable",
            "property",
            "enumMember",
            "event",
            "function",
            "method",
            "macro",
            "keyword",
            "modifier",
            "comment",
            "string",
            "number",
            "regexp",
            "operator",
        ];

        let token_modifiers = vec![
            "declaration",
            "definition",
            "readonly",
            "static",
            "deprecated",
            "abstract",
            "async",
            "modification",
            "documentation",
            "defaultLibrary",
        ];

        let workspace_folders = cfg
            .workspace_folders
            .iter()
            .map(|uri| json!({ "uri": uri, "name": uri }))
            .collect::<Vec<_>>();

        let init_params = json!({
            "processId": std::process::id(),
            "rootUri": cfg.root_uri,
            "workspaceFolders": workspace_folders.clone(),
            "capabilities": {
                "workspace": {
                    "configuration": true,
                    "workspaceFolders": true,
                },
                "textDocument": {
                    "hover": { "dynamicRegistration": false },
                    "completion": {
                        "dynamicRegistration": false,
                        "completionItem": { "snippetSupport": false },
                    },
                    "semanticTokens": {
                        "dynamicRegistration": false,
                        "requests": { "range": false, "full": { "delta": false } },
                        "tokenTypes": token_types,
                        "tokenModifiers": token_modifiers,
                        "formats": ["relative"],
                        "multilineTokenSupport": true,
                        "overlappingTokenSupport": false,
                    },
                    "foldingRange": {
                        "dynamicRegistration": false,
                        "lineFoldingOnly": true,
                    },
                    "definition": { "dynamicRegistration": false },
                    "declaration": { "dynamicRegistration": false },
                    "typeDefinition": { "dynamicRegistration": false },
                    "implementation": { "dynamicRegistration": false },
                    "references": { "dynamicRegistration": false },
                },
            },
            "clientInfo": { "name": "atto-ui editor" },
        });

        let start = editor_core_lsp::LspSessionStartOptions {
            cmd,
            workspace_folders,
            initialize_params: init_params,
            initialize_timeout: cfg.initialize_timeout,
            document: editor_core_lsp::LspDocument {
                uri: cfg.document_uri.clone(),
                language_id: cfg.language_id.clone(),
                version: 1,
            },
            initial_text: self.state_manager.editor().get_text(),
        };

        if let Ok(mut session) = editor_core_lsp::LspSession::start(start) {
            session.set_auto_refresh_options(editor_core_lsp::editor::LspAutoRefreshOptions {
                semantic_tokens: cfg.semantic_tokens,
                folding_ranges: cfg.folding_ranges,
                delay: Duration::from_millis(150),
            });
            self.lsp.session = Some(session);
        }
    }

    fn handle_lsp_hover_response(&mut self, value: &serde_json::Value, anchor: HoverAnchor) {
        if self.completion_popup.get().is_some() {
            return;
        }
        if self.lsp.hover_suppressed_position == Some(anchor.position) {
            return;
        }

        // LSP hover: { contents, range? }
        let Some(contents) = value.get("contents") else {
            self.hover_popup.set(None);
            return;
        };

        let text = match hover_contents_to_plain_text(contents) {
            Some(lines) if !lines.is_empty() => lines,
            _ => {
                self.hover_popup.set(None);
                return;
            }
        };

        let rect = self.hover_popup_rect_for_screen_point(anchor.screen, text.as_slice());

        self.hover_popup.set(Some(HoverPopupModel {
            rect,
            anchor: anchor.position,
            contents: crate::popup::LspHoverContents::PlainText(text),
        }));
    }

    fn handle_lsp_completion_response(&mut self, value: &serde_json::Value) {
        self.hide_hover_popup_only();
        if self
            .lsp
            .completion_requested_position
            .is_some_and(|p| p != self.active_cursor_position())
        {
            return;
        }

        // Completion: CompletionList { items } | CompletionItem[].
        let items_value = if let Some(items) = value.get("items") {
            items
        } else {
            value
        };

        let Some(arr) = items_value.as_array() else {
            self.completion_popup.set(None);
            return;
        };

        let max_items = self.config.completion.max_items.get().max(1);
        let mut items = Vec::new();
        for item in arr.iter().take(max_items) {
            let label = item
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if label.is_empty() {
                continue;
            }
            let detail = item
                .get("detail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            items.push(crate::popup::CompletionItem {
                label,
                detail,
                edit: LspCompletionItemEdit::Raw(item.clone()),
            });
        }

        if items.is_empty() {
            self.completion_popup.set(None);
            return;
        }

        let Some(rect) = self.completion_popup_rect_for_cursor(items.len()) else {
            self.completion_popup.set(None);
            return;
        };

        self.completion_popup.set(Some(CompletionPopupModel {
            rect,
            items,
            selected: 0,
            scroll: 0,
            accept: None,
        }));
    }

    pub(super) fn hover_popup_rect_for_screen_point(
        &self,
        screen: (u16, u16),
        lines: &[String],
    ) -> Rect {
        let (x, y) = screen;
        let max_line = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let width = (max_line + 2).clamp(10, 80) as u16;
        let height = (lines.len() + 2).clamp(3, 12) as u16;

        Rect {
            x: x.saturating_add(1),
            y: y.saturating_add(1),
            width,
            height,
        }
    }

    fn completion_popup_rect_for_cursor(&self, item_count: usize) -> Option<Rect> {
        let (cursor_x, cursor_y) = self.cursor_screen_position()??;
        let height = (item_count.min(8) + 2).max(3) as u16;
        let width = 40u16;
        Some(Rect {
            x: cursor_x.saturating_add(1),
            y: cursor_y.saturating_add(1),
            width,
            height,
        })
    }

    pub(super) fn cursor_screen_position(&self) -> Option<Option<(u16, u16)>> {
        let area = self.last_area?;
        let (_gutter, text_area) = self.layout_rects(area);
        if text_area.width == 0 || text_area.height == 0 {
            return Some(None);
        }

        let editor = self.state_manager.editor();
        let pos = self.active_cursor_position();
        let Some((cursor_visual_row, cursor_x_in_row)) =
            editor.logical_position_to_visual_allow_virtual(pos.line, pos.column)
        else {
            return Some(None);
        };
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        if cursor_visual_row < scroll_top {
            return Some(None);
        }
        let y = cursor_visual_row.saturating_sub(scroll_top);
        if y >= text_area.height as usize {
            return Some(None);
        }
        let x = cursor_x_in_row.min(text_area.width.saturating_sub(1) as usize) as u16;
        Some(Some((
            text_area.x.saturating_add(x),
            text_area.y.saturating_add(y as u16),
        )))
    }

    pub(super) fn layout_rects(&self, area: Rect) -> (Rect, Rect) {
        let show_line_numbers = self.config.show_line_numbers.get();
        let show_folding_markers = self.config.show_folding_markers.get();

        let line_count = self.state_manager.editor().line_index.line_count().max(1);
        let digits = line_count.to_string().len().max(2) as u16;

        let mut gutter_w = 0u16;
        if show_line_numbers {
            gutter_w = gutter_w.saturating_add(digits.saturating_add(1));
        }
        if show_folding_markers {
            gutter_w = gutter_w.saturating_add(2);
        }

        // Add a separator if there is any gutter at all.
        let sep_w = if gutter_w > 0 { 1 } else { 0 };
        let gutter_total = gutter_w.saturating_add(sep_w).min(area.width);

        let gutter = Rect {
            x: area.x,
            y: area.y,
            width: gutter_total,
            height: area.height,
        };
        let text = Rect {
            x: area.x.saturating_add(gutter_total),
            y: area.y,
            width: area.width.saturating_sub(gutter_total),
            height: area.height,
        };
        (gutter, text)
    }
}

// Helper to keep LSP hover parsing out of the main view module.
fn hover_contents_to_plain_text(contents: &serde_json::Value) -> Option<Vec<String>> {
    // Spec: `contents` can be MarkedString | MarkedString[] | MarkupContent.
    if let Some(s) = contents.as_str() {
        return Some(s.lines().map(|l| l.to_string()).collect());
    }

    if let Some(obj) = contents.as_object() {
        // MarkupContent: { kind: "markdown" | "plaintext", value: "..."}
        if let Some(value) = obj.get("value").and_then(|v| v.as_str()) {
            return Some(value.lines().map(|l| l.to_string()).collect());
        }
        // MarkedString: { language, value }
        if let Some(value) = obj.get("value").and_then(|v| v.as_str()) {
            return Some(value.lines().map(|l| l.to_string()).collect());
        }
    }

    if let Some(arr) = contents.as_array() {
        let mut out = Vec::<String>::new();
        for item in arr {
            if let Some(lines) = hover_contents_to_plain_text(item) {
                out.extend(lines);
            }
        }
        return Some(out);
    }

    None
}
