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
}
