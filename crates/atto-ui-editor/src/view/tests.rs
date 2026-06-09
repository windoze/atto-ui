use super::*;
use crate::EditorSyntaxConfig;
use atto_ui::composable::{Component, EventHandling};

fn buffer_row_string(
    terminal: &ratatui::Terminal<ratatui::backend::TestBackend>,
    y: u16,
) -> String {
    let buf = terminal.backend().buffer();
    let width = buf.area.width;
    let mut out = String::new();
    for x in 0..width {
        out.push_str(buf[(x, y)].symbol());
    }
    out
}

#[test]
fn editor_view_applies_simple_json_highlighting_on_new() {
    let text: atto_ui::reactive::Binding<String> = r#"{"s": "hello", "n": 42}"#.to_string().into();
    let cfg = EditorConfig::new(text);
    cfg.syntax.set(EditorSyntaxConfig::SimpleJson);

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (view, _handle) = EditorView::new(cfg, theme);

    // 'h' in "hello" is at column 7 in the sample.
    let offset = view
        .state_manager
        .editor()
        .line_index()
        .position_to_char_offset(0, 7);
    let styles = view.state_manager.get_styles_at(offset);

    assert!(
        styles.contains(&editor_core_highlight_simple::SIMPLE_STYLE_STRING),
        "expected SIMPLE_STYLE_STRING at \"hello\"; got {styles:?}"
    );
}

#[test]
fn editor_view_renders_simple_json_highlight_as_green_cells() {
    let text: atto_ui::reactive::Binding<String> = ["tab:ab", r#"{"s": "hello", "n": 42}"#, ""]
        .join("\n")
        .into();

    let cfg = EditorConfig::new(text);
    cfg.syntax.set(EditorSyntaxConfig::SimpleJson);

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);

    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = atto_ui::composable::ComponentContext {
        theme: &app_theme,
        window_id: atto_ui::wm::WindowId::default(),
        is_focused: true,
        scrollbar_host: atto_ui::composable::ScrollbarHost::Component,
        tab_mode: atto_ui::composable::TabMode::Cycle,
        mouse_coordinate_space: atto_ui::composable::MouseCoordinateSpace::Absolute,
        drag: None,
    };

    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 80, 10);
            view.draw(f, area, ctx);
        })
        .expect("draw");

    let buf = terminal.backend().buffer();

    // JSON line is the second document line; with default gutter enabled the text starts at
    // x=6. 'h' in "hello" is at column 7 in the JSON line.
    let x = 6 + 7;
    let y = 1;
    let cell = buf.cell((x as u16, y as u16));
    assert!(cell.is_some(), "expected buffer cell at ({x}, {y})");
    let cell = cell.unwrap();
    assert_eq!(cell.symbol(), "h", "expected to sample the 'h' in hello");
    assert_eq!(
        cell.style().fg,
        Some(ratatui::style::Color::Green),
        "expected syntax-highlighted string cell to be green"
    );
}

#[test]
fn editor_view_mouse_wheel_scrolls_even_at_viewport_edge() {
    let text: atto_ui::reactive::Binding<String> = (0..80)
        .map(|i| format!("LINE {:02}", i))
        .collect::<Vec<_>>()
        .join("\n")
        .into();

    let cfg = EditorConfig::new(text);

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);

    let backend = ratatui::backend::TestBackend::new(40, 8);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = atto_ui::composable::ComponentContext {
        theme: &app_theme,
        window_id: atto_ui::wm::WindowId::default(),
        is_focused: true,
        scrollbar_host: atto_ui::composable::ScrollbarHost::Component,
        tab_mode: atto_ui::composable::TabMode::Cycle,
        mouse_coordinate_space: atto_ui::composable::MouseCoordinateSpace::Absolute,
        drag: None,
    };

    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 40, 8);
            view.draw(f, area, ctx);
        })
        .expect("draw");

    let row0 = buffer_row_string(&terminal, 0);
    assert!(
        row0.contains("LINE 00"),
        "expected initial top line to be visible; got row0={row0:?}"
    );

    // Scroll while the pointer is over the text area.
    let (_gutter, text_area) = view.layout_rects(Rect::new(0, 0, 40, 8));
    let ev = Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: text_area.x.saturating_add(1),
        row: text_area.y,
        modifiers: KeyModifiers::NONE,
    });
    let _ = view.handle_event(&ev, ctx);

    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 40, 8);
            view.draw(f, area, ctx);
        })
        .expect("draw after scroll");

    let row0 = buffer_row_string(&terminal, 0);
    assert!(
        row0.contains("LINE 03"),
        "expected wheel scroll to advance content by 3 rows; got row0={row0:?}"
    );

    // Scroll again while the pointer is over the gutter (should still scroll).
    let ev = Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: text_area.y,
        modifiers: KeyModifiers::NONE,
    });
    let _ = view.handle_event(&ev, ctx);

    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 40, 8);
            view.draw(f, area, ctx);
        })
        .expect("draw after second scroll");

    let row0 = buffer_row_string(&terminal, 0);
    assert!(
        row0.contains("LINE 06"),
        "expected second wheel scroll to advance content again; got row0={row0:?}"
    );
}

fn test_view_with_text(text: &str) -> (EditorView, atto_ui::reactive::Binding<String>) {
    let text: atto_ui::reactive::Binding<String> = text.to_string().into();
    let cfg = EditorConfig::new(text.clone());
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (view, _handle) = EditorView::new(cfg, theme);
    (view, text)
}

fn test_view_with_config(cfg: EditorConfig) -> (EditorView, atto_ui::reactive::Binding<String>) {
    let text = cfg.text.clone();
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (view, _handle) = EditorView::new(cfg, theme);
    (view, text)
}

fn enable_auto_pairs(cfg: &EditorConfig) {
    let auto_pairs = editor_core::AutoPairsConfig {
        enabled: true,
        ..editor_core::AutoPairsConfig::default()
    };
    cfg.auto_pairs.set(auto_pairs);
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn test_ctx<'a>(theme: &'a atto_ui::theme::Theme) -> atto_ui::composable::ComponentContext<'a> {
    atto_ui::composable::ComponentContext {
        theme,
        window_id: atto_ui::wm::WindowId::default(),
        is_focused: true,
        scrollbar_host: atto_ui::composable::ScrollbarHost::Component,
        tab_mode: atto_ui::composable::TabMode::Cycle,
        mouse_coordinate_space: atto_ui::composable::MouseCoordinateSpace::Absolute,
        drag: None,
    }
}

#[test]
fn editor_typed_char_uses_editor_core_auto_pairs() {
    let text: atto_ui::reactive::Binding<String> = String::new().into();
    let cfg = EditorConfig::new(text);
    enable_auto_pairs(&cfg);
    let (mut view, text) = test_view_with_config(cfg);

    assert_eq!(
        view.handle_key_event(key(KeyCode::Char('('))),
        EventResult::consumed()
    );

    assert_eq!(text.get(), "()");
    assert_eq!(view.active_cursor_position(), Position::new(0, 1));
}

#[test]
fn editor_typed_pair_wraps_selection() {
    let text: atto_ui::reactive::Binding<String> = "abc".to_string().into();
    let cfg = EditorConfig::new(text);
    enable_auto_pairs(&cfg);
    let (mut view, text) = test_view_with_config(cfg);

    assert!(view.execute(Command::Cursor(CursorCommand::SetSelections {
        selections: vec![Selection {
            start: Position::new(0, 0),
            end: Position::new(0, 3),
            direction: SelectionDirection::Forward,
        }],
        primary_index: 0,
    })));
    assert_eq!(
        view.handle_key_event(key(KeyCode::Char('"'))),
        EventResult::consumed()
    );

    assert_eq!(text.get(), "\"abc\"");
    let selection = view
        .state_manager
        .editor()
        .selection()
        .cloned()
        .expect("wrapped selection");
    assert_eq!(selection.start, Position::new(0, 1));
    assert_eq!(selection.end, Position::new(0, 4));
}

#[test]
fn editor_insert_newline_uses_auto_indent_config() {
    let (mut view, text) = test_view_with_text("    let x = 1;");
    assert!(view.execute(Command::Cursor(CursorCommand::MoveToLineEnd)));

    assert!(view.handle_action(EditorAction::InsertNewline));

    assert_eq!(text.get(), "    let x = 1;\n    ");
    assert_eq!(view.active_cursor_position(), Position::new(1, 4));
}

#[test]
fn editor_unicode_type_char_is_not_changed_by_auto_pairs() {
    let text: atto_ui::reactive::Binding<String> = String::new().into();
    let cfg = EditorConfig::new(text);
    enable_auto_pairs(&cfg);
    let (mut view, text) = test_view_with_config(cfg);

    assert_eq!(
        view.handle_key_event(key(KeyCode::Char('界'))),
        EventResult::consumed()
    );

    assert_eq!(text.get(), "界");
    assert_eq!(view.active_cursor_position(), Position::new(0, 1));
}

#[test]
fn editor_paste_does_not_apply_auto_pairs() {
    let text: atto_ui::reactive::Binding<String> = String::new().into();
    let cfg = EditorConfig::new(text);
    enable_auto_pairs(&cfg);
    let (mut view, text) = test_view_with_config(cfg);
    let theme = atto_ui::theme::Theme::dark();

    assert_eq!(
        view.handle_event(&Event::Paste("(".to_string()), test_ctx(&theme)),
        EventResult::consumed()
    );

    assert_eq!(text.get(), "(");
    assert_eq!(view.active_cursor_position(), Position::new(0, 1));
}

#[test]
fn editor_read_only_blocks_type_char_and_insert_newline() {
    let text: atto_ui::reactive::Binding<String> = String::new().into();
    let cfg = EditorConfig::new(text);
    enable_auto_pairs(&cfg);
    cfg.read_only.set(true);
    let (mut view, text) = test_view_with_config(cfg);

    assert_eq!(
        view.handle_key_event(key(KeyCode::Char('('))),
        EventResult::ignored()
    );
    assert_eq!(
        view.handle_key_event(key(KeyCode::Enter)),
        EventResult::ignored()
    );
    assert_eq!(text.get(), "");
}

#[test]
fn editor_view_applies_diagnostics_to_summary_and_core_state() {
    let (mut view, _text) = test_view_with_text("let bad = 1;\n");
    let params = editor_core_lsp::LspPublishDiagnosticsParams {
        uri: "file:///diagnostics.rs".to_string(),
        version: Some(1),
        diagnostics: vec![editor_core_lsp::LspDiagnostic {
            range: editor_core_lsp::LspRange::new(
                editor_core_lsp::LspPosition::new(0, 4),
                editor_core_lsp::LspPosition::new(0, 7),
            ),
            severity: Some(editor_core_lsp::LspDiagnosticSeverity::Error),
            code: None,
            source: Some("test".to_string()),
            message: "mock error".to_string(),
            related_information: None,
            data: None,
        }],
    };

    view.apply_current_document_diagnostics(params);

    let summary = view.diagnostics_summary.get();
    assert_eq!(summary.errors, 1);
    assert_eq!(summary.warnings, 0);
    assert_eq!(summary.infos, 0);
    assert_eq!(summary.hints, 0);

    let diagnostics = view.state_manager.editor().diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "mock error");
}

#[test]
fn editor_view_renders_diagnostics_gutter_marker() {
    let (mut view, _text) = test_view_with_text("let bad = 1;\nlet ok = 2;\n");
    view.apply_current_document_diagnostics(editor_core_lsp::LspPublishDiagnosticsParams {
        uri: "file:///diagnostics.rs".to_string(),
        version: Some(1),
        diagnostics: vec![editor_core_lsp::LspDiagnostic {
            range: editor_core_lsp::LspRange::new(
                editor_core_lsp::LspPosition::new(0, 4),
                editor_core_lsp::LspPosition::new(0, 7),
            ),
            severity: Some(editor_core_lsp::LspDiagnosticSeverity::Error),
            code: None,
            source: Some("test".to_string()),
            message: "mock error".to_string(),
            related_information: None,
            data: None,
        }],
    });

    let backend = ratatui::backend::TestBackend::new(40, 4);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = atto_ui::composable::ComponentContext {
        theme: &app_theme,
        window_id: atto_ui::wm::WindowId::default(),
        is_focused: true,
        scrollbar_host: atto_ui::composable::ScrollbarHost::Component,
        tab_mode: atto_ui::composable::TabMode::Cycle,
        mouse_coordinate_space: atto_ui::composable::MouseCoordinateSpace::Absolute,
        drag: None,
    };

    terminal
        .draw(|f| view.draw(f, Rect::new(0, 0, 40, 4), ctx))
        .expect("draw");

    let buf = terminal.backend().buffer();
    let marker = buf.cell((5, 0)).expect("diagnostic marker cell");
    assert_eq!(marker.symbol(), "E");
    assert_eq!(marker.style().fg, Some(ratatui::style::Color::LightRed));
    assert_eq!(buf.cell((7, 0)).expect("separator").symbol(), "│");
    assert_eq!(buf.cell((8, 0)).expect("text start").symbol(), "l");
}

#[test]
fn editor_view_does_not_repeat_diagnostic_marker_on_wrapped_rows() {
    let (mut view, _text) = test_view_with_text("abcdefghijk\n");
    view.apply_current_document_diagnostics(editor_core_lsp::LspPublishDiagnosticsParams {
        uri: "file:///diagnostics.rs".to_string(),
        version: Some(1),
        diagnostics: vec![editor_core_lsp::LspDiagnostic {
            range: editor_core_lsp::LspRange::new(
                editor_core_lsp::LspPosition::new(0, 0),
                editor_core_lsp::LspPosition::new(0, 3),
            ),
            severity: Some(editor_core_lsp::LspDiagnosticSeverity::Error),
            code: None,
            source: Some("test".to_string()),
            message: "mock error".to_string(),
            related_information: None,
            data: None,
        }],
    });

    let backend = ratatui::backend::TestBackend::new(12, 4);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = atto_ui::composable::ComponentContext {
        theme: &app_theme,
        window_id: atto_ui::wm::WindowId::default(),
        is_focused: true,
        scrollbar_host: atto_ui::composable::ScrollbarHost::Component,
        tab_mode: atto_ui::composable::TabMode::Cycle,
        mouse_coordinate_space: atto_ui::composable::MouseCoordinateSpace::Absolute,
        drag: None,
    };

    terminal
        .draw(|f| view.draw(f, Rect::new(0, 0, 12, 4), ctx))
        .expect("draw");

    let buf = terminal.backend().buffer();
    assert_eq!(buf.cell((5, 0)).expect("row 0 marker").symbol(), "E");
    assert_eq!(buf.cell((5, 1)).expect("row 1 marker").symbol(), " ");
    assert_eq!(buf.cell((7, 1)).expect("row 1 separator").symbol(), "│");
    assert_eq!(buf.cell((8, 1)).expect("wrapped text start").symbol(), "e");
}

#[test]
fn editor_actions_jump_between_diagnostics_with_wraparound() {
    let (mut view, _text) = test_view_with_text("zero\none\ntwo\nthree\n");
    view.apply_current_document_diagnostics(editor_core_lsp::LspPublishDiagnosticsParams {
        uri: "file:///diagnostics.rs".to_string(),
        version: Some(1),
        diagnostics: vec![
            editor_core_lsp::LspDiagnostic {
                range: editor_core_lsp::LspRange::new(
                    editor_core_lsp::LspPosition::new(1, 0),
                    editor_core_lsp::LspPosition::new(1, 3),
                ),
                severity: Some(editor_core_lsp::LspDiagnosticSeverity::Warning),
                code: None,
                source: Some("test".to_string()),
                message: "warning".to_string(),
                related_information: None,
                data: None,
            },
            editor_core_lsp::LspDiagnostic {
                range: editor_core_lsp::LspRange::new(
                    editor_core_lsp::LspPosition::new(3, 0),
                    editor_core_lsp::LspPosition::new(3, 5),
                ),
                severity: Some(editor_core_lsp::LspDiagnosticSeverity::Error),
                code: None,
                source: Some("test".to_string()),
                message: "error".to_string(),
                related_information: None,
                data: None,
            },
        ],
    });

    let _ = view.execute(Command::Cursor(CursorCommand::MoveTo {
        line: 0,
        column: 0,
    }));
    assert!(view.handle_action(EditorAction::LspNextDiagnostic));
    assert_eq!(view.active_cursor_position(), Position::new(1, 0));

    assert!(view.handle_action(EditorAction::LspNextDiagnostic));
    assert_eq!(view.active_cursor_position(), Position::new(3, 0));

    assert!(view.handle_action(EditorAction::LspNextDiagnostic));
    assert_eq!(view.active_cursor_position(), Position::new(1, 0));

    assert!(view.handle_action(EditorAction::LspPrevDiagnostic));
    assert_eq!(view.active_cursor_position(), Position::new(3, 0));
}

#[test]
fn editor_lsp_document_symbols_response_emits_utf16_converted_outline() {
    let text: atto_ui::reactive::Binding<String> = "fn 👋target() {}\n".to_string().into();
    let cfg = EditorConfig::new(text);
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);

    view.lsp.pending_document_symbols = Some(42);
    view.handle_lsp_response(editor_core_lsp::LspResponse {
        id: 42,
        method: "textDocument/documentSymbol".to_string(),
        result: Some(serde_json::json!([
            {
                "name": "target",
                "kind": 12,
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 17 }
                },
                "selectionRange": {
                    "start": { "line": 0, "character": 5 },
                    "end": { "line": 0, "character": 11 }
                }
            }
        ])),
        error: None,
    });

    let events = handle.events.drain();
    let [EditorEvent::DocumentSymbols { outline }] = events.as_slice() else {
        panic!("expected document symbol event, got {events:?}");
    };
    let symbols = outline.flatten_preorder();
    assert_eq!(symbols.len(), 1);
    let (line, column) = view
        .state_manager
        .editor()
        .line_index()
        .char_offset_to_position(symbols[0].selection_range.start);
    assert_eq!((line, column), (0, 4));
}

#[test]
fn stale_completion_response_does_not_clear_signature_help_popup() {
    let text: atto_ui::reactive::Binding<String> = "mock_fn(".to_string().into();
    let cfg = EditorConfig::new(text);
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);

    view.lsp.completion_pending_request = Some(42);
    view.lsp.completion_requested_position = Some(Position::new(0, 0));
    let _ = view.execute(Command::Cursor(CursorCommand::MoveTo {
        line: 0,
        column: 8,
    }));
    handle
        .signature_help_popup
        .set(Some(SignatureHelpPopupModel {
            rect: Rect::new(2, 2, 32, 3),
            signatures: vec![editor_core_lsp::LspSignatureInformation {
                label: "mock_fn(arg: i32)".to_string(),
                documentation: None,
                parameters: Vec::new(),
            }],
            active_signature: Some(0),
            active_parameter: Some(0),
        }));

    view.handle_lsp_response(editor_core_lsp::LspResponse {
        id: 42,
        method: "textDocument/completion".to_string(),
        result: Some(serde_json::json!([{ "label": "stale_completion" }])),
        error: None,
    });

    assert!(handle.completion_popup.get().is_none());
    assert!(handle.signature_help_popup.get().is_some());
    assert!(view.lsp.completion_pending_request.is_none());
    assert!(view.lsp.completion_requested_position.is_none());
}

#[test]
fn editor_jump_to_utf16_position_converts_to_char_column() {
    let text: atto_ui::reactive::Binding<String> = "fn 👋target() {}\n".to_string().into();
    let cfg = EditorConfig::new(text);
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);

    assert!(view.jump_to_utf16_position(0, 5));
    assert_eq!(view.active_cursor_position(), Position::new(0, 4));
}

#[test]
fn clear_lsp_diagnostics_clears_all_controller_state() {
    let (mut view, _text) = test_view_with_text("let bad = 1;\n");
    view.lsp.diagnostics.push(editor_core_lsp::LspDiagnostic {
        range: editor_core_lsp::LspRange::new(
            editor_core_lsp::LspPosition::new(0, 4),
            editor_core_lsp::LspPosition::new(0, 7),
        ),
        severity: Some(editor_core_lsp::LspDiagnosticSeverity::Error),
        code: None,
        source: Some("test".to_string()),
        message: "mock error".to_string(),
        related_information: None,
        data: None,
    });
    view.lsp.diagnostic_result_id = Some("previous".to_string());
    view.lsp.pending_document_diagnostic = Some(42);
    view.lsp.diagnostic_cursor = Some(0);
    view.diagnostics_summary.set(DiagnosticsSummary {
        errors: 1,
        warnings: 0,
        infos: 0,
        hints: 0,
    });

    let revision_before = view.lsp.diagnostics_revision;
    view.clear_lsp_diagnostics();

    assert!(view.lsp.diagnostics.is_empty());
    assert_eq!(view.lsp.diagnostic_result_id, None);
    assert_eq!(view.lsp.pending_document_diagnostic, None);
    assert_eq!(view.lsp.diagnostic_cursor, None);
    assert_eq!(
        view.diagnostics_summary.get(),
        DiagnosticsSummary::default()
    );
    assert_eq!(view.lsp.diagnostics_revision, revision_before + 1);
}

#[test]
fn editor_actions_line_edits_sync_text_binding() {
    let text: atto_ui::reactive::Binding<String> = "a\nb\nc".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.indent.tab_width.set(2);
    cfg.indent.insert_spaces.set(true);
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);

    let _ = view.execute(Command::Cursor(CursorCommand::MoveTo {
        line: 1,
        column: 0,
    }));
    assert!(view.handle_action(EditorAction::Indent));
    assert_eq!(text.get(), "a\n  b\nc");

    assert!(view.handle_action(EditorAction::Outdent));
    assert_eq!(text.get(), "a\nb\nc");

    let _ = view.execute(Command::Cursor(CursorCommand::MoveTo {
        line: 1,
        column: 0,
    }));
    assert!(view.handle_action(EditorAction::DuplicateLines));
    assert_eq!(text.get(), "a\nb\nb\nc");

    assert!(view.handle_action(EditorAction::DeleteLines));
    assert_eq!(text.get(), "a\nb\nc");

    let _ = view.execute(Command::Cursor(CursorCommand::MoveTo {
        line: 1,
        column: 0,
    }));
    assert!(view.handle_action(EditorAction::MoveLinesUp));
    assert_eq!(text.get(), "b\na\nc");

    assert!(view.handle_action(EditorAction::MoveLinesDown));
    assert_eq!(text.get(), "a\nb\nc");
}

#[test]
fn editor_actions_comment_join_and_split_sync_text_binding() {
    let text: atto_ui::reactive::Binding<String> = "let x = 1;\n  let y = 2;".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.comment
        .set(Some(editor_core::CommentConfig::line("//")));
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);

    assert!(view.handle_action(EditorAction::ToggleComment));
    assert_eq!(text.get(), "// let x = 1;\n  let y = 2;");

    assert!(view.handle_action(EditorAction::ToggleComment));
    assert_eq!(text.get(), "let x = 1;\n  let y = 2;");

    assert!(view.handle_action(EditorAction::JoinLines));
    assert_eq!(text.get(), "let x = 1; let y = 2;");

    let (mut split_view, split_text) = test_view_with_text("hello world");
    let _ = split_view.execute(Command::Cursor(CursorCommand::MoveTo {
        line: 0,
        column: 5,
    }));
    assert!(split_view.handle_action(EditorAction::SplitLine));
    assert_eq!(split_text.get(), "hello\n world");
}

#[test]
fn editor_actions_multi_cursor_occurrence_commands() {
    let (mut view, _text) = test_view_with_text("foo foo foo");
    let _ = view.execute(Command::Cursor(CursorCommand::MoveTo {
        line: 0,
        column: 0,
    }));

    assert!(view.handle_action(EditorAction::AddNextOccurrence));
    let sel = view
        .state_manager
        .editor()
        .selection()
        .cloned()
        .expect("primary selection");
    assert_eq!(sel.start, Position::new(0, 4));
    assert_eq!(sel.end, Position::new(0, 7));
    assert_eq!(view.state_manager.editor().secondary_selections().len(), 1);

    let (mut view, _text) = test_view_with_text("foo foo foo");
    let _ = view.execute(Command::Cursor(CursorCommand::MoveTo {
        line: 0,
        column: 0,
    }));

    assert!(view.handle_action(EditorAction::AddAllOccurrences));
    assert_eq!(view.state_manager.editor().secondary_selections().len(), 2);
}

#[test]
fn editor_actions_read_only_blocks_document_mutations() {
    let text: atto_ui::reactive::Binding<String> = "a\nb".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.read_only.set(true);
    cfg.comment
        .set(Some(editor_core::CommentConfig::line("//")));
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);

    assert!(!view.handle_action(EditorAction::DeleteLines));
    assert!(!view.handle_action(EditorAction::ToggleComment));
    assert_eq!(text.get(), "a\nb");

    assert!(view.handle_action(EditorAction::MoveWordRight));
}

#[test]
fn editor_default_keymap_includes_stage_three_actions_without_known_conflicts() {
    let keymap = EditorKeymap::default_bindings();

    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::Left, KeyModifiers::CONTROL)),
        Some(EditorAction::MoveWordLeft)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::Right, KeyModifiers::CONTROL)),
        Some(EditorAction::MoveWordRight)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::Char('/'), KeyModifiers::CONTROL)),
        Some(EditorAction::ToggleComment)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::Char('7'), KeyModifiers::CONTROL)),
        Some(EditorAction::ToggleComment)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::Down, KeyModifiers::ALT)),
        Some(EditorAction::MoveLinesDown)
    );
    assert_eq!(
        keymap.get(KeyChord::new(
            KeyCode::Down,
            KeyModifiers::ALT | KeyModifiers::SHIFT
        )),
        Some(EditorAction::DuplicateLines)
    );
    assert_eq!(
        keymap.get(KeyChord::new(
            KeyCode::Down,
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        )),
        Some(EditorAction::AddCursorBelow)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        Some(EditorAction::AddNextOccurrence)
    );
    assert_eq!(
        keymap.get(KeyChord::new(
            KeyCode::Char('l'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )),
        Some(EditorAction::AddAllOccurrences)
    );

    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Some(EditorAction::Copy)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::Char('f'), KeyModifiers::CONTROL)),
        Some(EditorAction::Find)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::F(12), KeyModifiers::NONE)),
        Some(EditorAction::LspGotoDefinition)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::F(8), KeyModifiers::NONE)),
        Some(EditorAction::LspNextDiagnostic)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::F(8), KeyModifiers::SHIFT)),
        Some(EditorAction::LspPrevDiagnostic)
    );
    assert_eq!(
        keymap.get(KeyChord::new(KeyCode::F(2), KeyModifiers::NONE)),
        Some(EditorAction::LspRename)
    );
}
