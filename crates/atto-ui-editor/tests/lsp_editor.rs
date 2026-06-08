use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use atto_ui::composable::{
    Component, ComponentContext, EventHandling, MouseCoordinateSpace, ScrollbarHost, TabMode,
};
use atto_ui::wm::WindowId;
use atto_ui_editor::{
    CodeActionItemView, CodeActionPopupModel, CompletionItem, CompletionPopupModel,
    DiagnosticsSummary, EditorAction, EditorConfig, EditorEvent, EditorLspConfig, EditorLspMode,
    EditorSyntaxConfig, EditorViewHandle, LspCompletionItemEdit, SignatureHelpPopupModel,
};
use atto_ui_editor::{EditorThemeSet, EditorView};

#[test]
fn lsp_semantic_tokens_and_folding_markers_render_and_toggle() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text = "fn main() {\n    let s = \"hello\";\n}\n".to_string();
    let cfg = EditorConfig::new(text);
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);

    let lsp_cfg = EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///mock.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: true,
        folding_ranges: true,
    };
    cfg.lsp.set(EditorLspMode::Enabled(lsp_cfg));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);

    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let area = Rect::new(0, 0, 80, 10);

    // Wait for LSP-derived semantic tokens + folding ranges to land.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        let buf = terminal.backend().buffer();

        // With 3 lines and default gutter config: fold marker is at (3,0), text begins at x=6.
        let fold_marker = buf.cell((3, 0)).expect("fold marker cell");
        let string_h = buf.cell((19, 1)).expect("string token cell");

        if fold_marker.symbol() == "▼"
            && string_h.symbol() == "h"
            && string_h.style().fg == Some(Color::Green)
        {
            break;
        }

        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for LSP semantic tokens + folding ranges; fold_marker='{}' string_h='{}' fg={:?}",
                fold_marker.symbol(),
                string_h.symbol(),
                string_h.style().fg
            );
        }

        thread::sleep(Duration::from_millis(10));
    }

    // Click fold marker in the gutter.
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    view.handle_event(&click, ctx);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("draw after fold");
    let buf = terminal.backend().buffer();
    let folded = buf.cell((3, 0)).expect("fold marker after click");
    assert_eq!(folded.symbol(), "▶");

    // Click again should unfold.
    view.handle_event(&click, ctx);
    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("draw after unfold");
    let buf = terminal.backend().buffer();
    let unfolded = buf.cell((3, 0)).expect("fold marker after second click");
    assert_eq!(unfolded.symbol(), "▼");
}

#[test]
fn lsp_hover_popup_tracks_mouse_and_suppresses_until_move() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text = "hover test line\nsecond line\nthird line\n".to_string();
    let cfg = EditorConfig::new(text);
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(true);
    // Make hover tests fast + deterministic.
    cfg.hover.delay.set(Duration::from_millis(0));

    let lsp_cfg = EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///mock.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    };
    cfg.lsp.set(EditorLspMode::Enabled(lsp_cfg));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);

    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let area = Rect::new(0, 0, 80, 10);

    // Ensure `last_area` is set so mouse events are accepted.
    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");

    // Move mouse over the text area; with default gutter config, text begins at x=6.
    let mouse = (10u16, 1u16);
    let moved = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: mouse.0,
        row: mouse.1,
        modifiers: KeyModifiers::NONE,
    });
    view.handle_event(&moved, ctx);

    // Wait for the hover popup to show.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        if let Some(model) = handle.hover_popup.get() {
            assert_eq!(model.anchor.line, 1);
            assert_eq!(model.rect.x, mouse.0 + 1);
            assert_eq!(model.rect.y, mouse.1 + 1);
            break;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for hover popup model");
        }
        thread::sleep(Duration::from_millis(10));
    }

    // Esc should close the hover popup and suppress it from re-opening at the same mouse position.
    let esc = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    view.handle_event(&esc, ctx);
    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("draw after esc");
    assert!(handle.hover_popup.get().is_none());

    // Trigger re-scheduling via a harmless keypress; popup should remain suppressed.
    let left = Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    view.handle_event(&left, ctx);

    let suppress_deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < suppress_deadline {
        terminal
            .draw(|f| view.draw(f, area, ctx))
            .expect("draw suppressed");
        if handle.hover_popup.get().is_some() {
            panic!("hover popup re-opened without mouse movement");
        }
        thread::sleep(Duration::from_millis(10));
    }

    // Moving the mouse to a new position should clear suppression and allow the popup again.
    let mouse2 = (14u16, 1u16);
    let moved2 = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: mouse2.0,
        row: mouse2.1,
        modifiers: KeyModifiers::NONE,
    });
    view.handle_event(&moved2, ctx);

    let deadline2 = Instant::now() + Duration::from_secs(2);
    loop {
        terminal
            .draw(|f| view.draw(f, area, ctx))
            .expect("draw after move");
        if let Some(model) = handle.hover_popup.get() {
            assert_eq!(model.rect.x, mouse2.0 + 1);
            assert_eq!(model.rect.y, mouse2.1 + 1);
            break;
        }
        if Instant::now() >= deadline2 {
            panic!("timed out waiting for hover popup after mouse move");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn lsp_publish_diagnostics_updates_summary() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text = "let bad = 1;\n".to_string();
    let cfg = EditorConfig::new(text);
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);

    let lsp_cfg = EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///diagnostics.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    };
    cfg.lsp.set(EditorLspMode::Enabled(lsp_cfg));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);

    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let area = Rect::new(0, 0, 80, 10);
    assert_eq!(
        handle.diagnostics_summary.get(),
        DiagnosticsSummary::default()
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        let summary = handle.diagnostics_summary.get();
        if summary.errors == 1 {
            assert_eq!(summary.warnings, 0);
            assert_eq!(summary.infos, 0);
            assert_eq!(summary.hints, 0);
            let buf = terminal.backend().buffer();
            assert_eq!(buf.cell((5, 0)).expect("diagnostic marker").symbol(), "E");
            assert_eq!(buf.cell((8, 0)).expect("text start").symbol(), "l");
            break;
        }

        if Instant::now() >= deadline {
            panic!("timed out waiting for diagnostics summary; got {summary:?}");
        }

        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn lsp_inlay_hints_render_as_virtual_text_and_toggle_off() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let original = "let value = 1;\n";
    let text: atto_ui::reactive::Binding<String> = original.to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.inlay_hints.enabled.set(true);
    cfg.inlay_hints.refresh_delay.set(Duration::from_millis(0));
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///inlay.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        let buf = terminal.backend().buffer();
        let text_start = 6u16;
        if buf.cell((text_start + 9, 0)).expect("inlay colon").symbol() == ":"
            && buf.cell((text_start + 11, 0)).expect("inlay type").symbol() == "i"
        {
            assert_eq!(text.get(), original);
            break;
        }

        if Instant::now() >= deadline {
            panic!("timed out waiting for inlay hint render");
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(view.handle_editor_action(EditorAction::LspToggleInlayHints));
    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("draw after inlay toggle off");
    let buf = terminal.backend().buffer();
    let text_start = 6u16;
    assert_eq!(
        buf.cell((text_start + 9, 0))
            .expect("original space after toggle")
            .symbol(),
        " "
    );
    assert_eq!(
        buf.cell((text_start + 10, 0))
            .expect("original equals after toggle")
            .symbol(),
        "="
    );
    assert_eq!(text.get(), original);
}

#[test]
fn lsp_inlay_hints_preserve_semantic_styles_and_copy_backing_text() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let original = "fn main() {\n    let s = \"hello\";\n}\n";
    let text: atto_ui::reactive::Binding<String> = original.to_string().into();
    let cfg = EditorConfig::new(text.clone());
    let clipboard = cfg.clipboard.clone();
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.inlay_hints.enabled.set(true);
    cfg.inlay_hints.refresh_delay.set(Duration::from_millis(0));
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///mock.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: true,
        folding_ranges: true,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, _handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        let buf = terminal.backend().buffer();
        let text_start = 6u16;
        let inlay_colon = buf.cell((text_start + 9, 0)).expect("inlay colon");
        let string_h = buf.cell((text_start + 13, 1)).expect("string token cell");
        let fold_marker = buf.cell((3, 0)).expect("fold marker cell");

        if inlay_colon.symbol() == ":"
            && string_h.symbol() == "h"
            && string_h.style().fg == Some(Color::Green)
            && fold_marker.symbol() == "▼"
        {
            assert!(view.handle_editor_action(EditorAction::SelectAll));
            assert!(view.handle_editor_action(EditorAction::Copy));
            assert_eq!(clipboard.get(), original);
            assert_eq!(text.get(), original);
            break;
        }

        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for inlay + semantic/folding render; inlay='{}' string_h='{}' fg={:?} fold='{}'",
                inlay_colon.symbol(),
                string_h.symbol(),
                string_h.style().fg,
                fold_marker.symbol()
            );
        }

        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn lsp_signature_help_popup_triggers_after_open_paren_and_esc_closes() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "mock_fn".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///signature.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    view.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
        ctx,
    );
    view.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('('), KeyModifiers::NONE)),
        ctx,
    );
    assert_eq!(text.get(), "mock_fn(");

    let popup = wait_for_signature_help_popup(&mut terminal, &mut view, &handle, area, ctx);
    assert_eq!(popup.signatures[0].label, "mock_fn(arg: i32, next: i32)");
    assert_eq!(popup.active_signature, Some(0));
    assert_eq!(popup.active_parameter, Some(0));

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("draw signature help");
    let buf = terminal.backend().buffer();
    let active_cell = buf
        .cell((popup.rect.x + 1 + 8, popup.rect.y + 1))
        .expect("active parameter cell");
    assert_eq!(active_cell.symbol(), "a");
    assert!(
        active_cell
            .style()
            .add_modifier
            .contains(Modifier::UNDERLINED)
    );

    view.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        ctx,
    );
    assert!(handle.signature_help_popup.get().is_none());
}

#[test]
fn lsp_signature_help_manual_action_clears_empty_result() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "mock_fn(".to_string().into();
    let cfg = EditorConfig::new(text);
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///signature_empty.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    handle
        .signature_help_popup
        .set(Some(SignatureHelpPopupModel {
            rect: Rect::new(2, 2, 20, 3),
            signatures: vec![editor_core_lsp::LspSignatureInformation {
                label: "stale(arg)".to_string(),
                documentation: None,
                parameters: Vec::new(),
            }],
            active_signature: Some(0),
            active_parameter: None,
        }));

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    assert!(view.handle_editor_action(EditorAction::LspSignatureHelp));
    wait_for_signature_help_popup_to_clear(&mut terminal, &mut view, &handle, area, ctx);
}

#[test]
fn lsp_signature_help_response_does_not_override_completion_popup() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "mock_fn".to_string().into();
    let cfg = EditorConfig::new(text);
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///signature.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    assert!(view.handle_editor_action(EditorAction::LspSignatureHelp));
    handle.completion_popup.set(Some(CompletionPopupModel {
        rect: Rect::new(2, 2, 20, 3),
        items: vec![CompletionItem {
            label: "mock_fn".to_string(),
            detail: None,
            edit: LspCompletionItemEdit::Raw(serde_json::json!({ "label": "mock_fn" })),
        }],
        selected: 0,
        scroll: 0,
        accept: None,
    }));

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        assert!(
            handle.signature_help_popup.get().is_none(),
            "signature popup should not appear over completion"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(handle.completion_popup.get().is_some());
}

#[test]
fn lsp_code_action_popup_displays_and_applies_single_document_edit() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "let bad = 1;\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///code_action.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    let code_action = Event::Key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::CONTROL));
    view.handle_event(&code_action, ctx);

    wait_for_code_action_popup(&mut terminal, &mut view, &handle, area, ctx);
    let popup = handle.code_action_popup.get().expect("code action popup");
    assert_eq!(popup.items[0].title, "Replace bad with good");
    assert_eq!(popup.items[0].kind.as_deref(), Some("quickfix"));
    assert!(popup.items[0].is_preferred);

    let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    view.handle_event(&enter, ctx);
    assert_eq!(text.get(), "let good = 1;\n");

    let undo = Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    view.handle_event(&undo, ctx);
    assert_eq!(text.get(), "let bad = 1;\n");
}

#[test]
fn lsp_code_action_cross_file_edit_is_reported_and_not_applied() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "let bad = 1;\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///code_action_cross.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    let code_action = Event::Key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::CONTROL));
    view.handle_event(&code_action, ctx);

    wait_for_code_action_popup(&mut terminal, &mut view, &handle, area, ctx);
    let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    view.handle_event(&enter, ctx);

    assert_eq!(text.get(), "let bad = 1;\n");
    let messages = handle.events.drain();
    assert!(
        messages.iter().any(|event| matches!(
            event,
            EditorEvent::CodeActionMessage { message }
                if message.contains("Skipped code action") && message.contains("file:///other.rs")
        )),
        "expected cross-file skip event, got {messages:?}"
    );
}

#[test]
fn lsp_code_action_command_without_edit_executes() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "let bad = 1;\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///code_action_command.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    let code_action = Event::Key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::CONTROL));
    view.handle_event(&code_action, ctx);

    wait_for_code_action_popup(&mut terminal, &mut view, &handle, area, ctx);
    let popup = handle.code_action_popup.get().expect("code action popup");
    assert_eq!(popup.items[0].title, "Run mock command");

    let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    view.handle_event(&enter, ctx);
    assert_eq!(text.get(), "let bad = 1;\n");

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        let summary = handle.diagnostics_summary.get();
        if summary.infos == 1 {
            assert_eq!(summary.errors, 0);
            assert_eq!(summary.warnings, 0);
            assert_eq!(summary.hints, 0);
            break;
        }

        if Instant::now() >= deadline {
            panic!("timed out waiting for command-only code action execution; got {summary:?}");
        }

        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn lsp_format_document_applies_edits_and_undoes_as_single_step() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let original = "let bad = 1;\nlet worse = 2;\n";
    let text: atto_ui::reactive::Binding<String> = original.to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///formatting_multi.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    assert!(
        view.handle_editor_action(EditorAction::LspFormatDocument),
        "format request events: {:?}",
        handle.events.drain()
    );

    let (success, changed) = wait_for_format_finished(&mut terminal, &mut view, &handle, area, ctx);
    assert!(success);
    assert!(changed);
    assert_eq!(text.get(), "let good = 1;\nlet better = 2;\n");

    assert!(view.handle_editor_action(EditorAction::Undo));
    assert_eq!(text.get(), original);
}

#[test]
fn lsp_format_document_uses_current_indentation_options() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "options\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.indent.tab_width.set(2);
    cfg.indent.insert_spaces.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///formatting_options.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    assert!(
        view.handle_editor_action(EditorAction::LspFormatDocument),
        "format request events: {:?}",
        handle.events.drain()
    );
    let (success, changed) = wait_for_format_finished(&mut terminal, &mut view, &handle, area, ctx);

    assert!(success);
    assert!(changed);
    assert_eq!(text.get(), "tabSize=2 insertSpaces=false\n");
}

#[test]
fn lsp_format_document_ctrl_k_ctrl_f_sequence_triggers_formatting() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "unformatted\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///formatting.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    assert!(
        view.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            ctx,
        )
        .is_consumed()
    );
    assert!(
        view.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)),
            ctx,
        )
        .is_consumed()
    );

    let (success, changed) = wait_for_format_finished(&mut terminal, &mut view, &handle, area, ctx);
    assert!(success);
    assert!(changed);
    assert_eq!(text.get(), "formatted\n");
}

#[test]
fn lsp_format_document_empty_edits_leave_text_unchanged() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let original = "unchanged\n";
    let text: atto_ui::reactive::Binding<String> = original.to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///formatting_empty.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    assert!(
        view.handle_editor_action(EditorAction::LspFormatDocument),
        "format request events: {:?}",
        handle.events.drain()
    );
    let (success, changed) = wait_for_format_finished(&mut terminal, &mut view, &handle, area, ctx);

    assert!(success);
    assert!(!changed);
    assert_eq!(text.get(), original);
    assert!(!view.handle_editor_action(EditorAction::Undo));
    assert_eq!(text.get(), original);
}

#[test]
fn lsp_format_document_error_reports_message() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "bad\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///formatting_error.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    assert!(view.handle_editor_action(EditorAction::LspFormatDocument));
    let message = wait_for_lsp_message(
        &mut terminal,
        &mut view,
        &handle,
        area,
        ctx,
        "Formatting failed: mock formatting error",
    );

    assert!(message.contains("Formatting failed: mock formatting error"));
    assert_eq!(text.get(), "bad\n");
}

#[test]
fn lsp_format_document_transport_exit_times_out_and_finishes() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "bad\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.formatting_timeout.set(Duration::from_millis(50));
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///formatting_disconnect.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    assert!(view.handle_editor_action(EditorAction::LspFormatDocument));
    let (message, success, changed) = wait_for_lsp_message_and_format_finished(
        &mut terminal,
        &mut view,
        &handle,
        area,
        ctx,
        "Formatting timed out",
    );

    assert!(message.contains("Formatting timed out"));
    assert!(!success);
    assert!(!changed);
    assert_eq!(text.get(), "bad\n");
}

#[test]
fn lsp_format_document_without_lsp_is_ignored() {
    let text: atto_ui::reactive::Binding<String> = "plain\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);

    assert!(!view.handle_editor_action(EditorAction::LspFormatDocument));
    assert_eq!(text.get(), "plain\n");
    assert!(handle.events.drain().is_empty());
}

#[test]
fn lsp_rename_popup_submits_workspace_edit_event() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "let bad = 1;\n".to_string().into();
    let cfg = EditorConfig::new(text.clone());
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///rename.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    let rename = Event::Key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    view.handle_event(&rename, ctx);

    wait_for_rename_popup(&mut terminal, &mut view, &handle, area, ctx);
    let popup = handle.rename_popup.get().expect("rename popup");
    assert_eq!(popup.value, "bad");
    assert!(popup.replace_on_input);

    for ch in "good".chars() {
        view.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
            ctx,
        );
    }
    let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    view.handle_event(&enter, ctx);

    let edit = wait_for_rename_workspace_edit(&mut terminal, &mut view, &handle, area, ctx);
    assert_eq!(text.get(), "let bad = 1;\n");
    assert_eq!(
        edit.pointer("/changes/file:~1~1~1rename.rs/0/newText")
            .and_then(serde_json::Value::as_str),
        Some("good")
    );
}

#[test]
fn lsp_rename_prepare_null_reports_message_without_popup() {
    assert_rename_prepare_message(
        "file:///rename_unavailable.rs",
        "Rename is not available at the cursor",
    );
}

#[test]
fn lsp_rename_prepare_error_reports_message_without_popup() {
    assert_rename_prepare_message(
        "file:///rename_error.rs",
        "Rename is not available: mock prepare rename error",
    );
}

#[test]
fn lsp_rename_request_clears_completion_and_code_action_popups() {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "let bad = 1;\n".to_string().into();
    let cfg = EditorConfig::new(text);
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: "file:///rename.rs".to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    handle.completion_popup.set(Some(CompletionPopupModel {
        rect: Rect::new(2, 2, 20, 3),
        items: vec![CompletionItem {
            label: "bad".to_string(),
            detail: None,
            edit: LspCompletionItemEdit::Raw(serde_json::json!({ "label": "bad" })),
        }],
        selected: 0,
        scroll: 0,
        accept: None,
    }));
    handle.code_action_popup.set(Some(CodeActionPopupModel {
        rect: Rect::new(2, 5, 24, 3),
        items: vec![CodeActionItemView {
            title: "Replace bad with good".to_string(),
            kind: Some("quickfix".to_string()),
            is_preferred: true,
        }],
        selected: 0,
        scroll: 0,
        accept: None,
    }));

    let rename = Event::Key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    view.handle_event(&rename, ctx);

    assert!(handle.completion_popup.get().is_none());
    assert!(handle.code_action_popup.get().is_none());
    wait_for_rename_popup(&mut terminal, &mut view, &handle, area, ctx);
    assert!(handle.completion_popup.get().is_none());
    assert!(handle.code_action_popup.get().is_none());
}

fn assert_rename_prepare_message(document_uri: &str, expected: &str) {
    let server_bin = env!("CARGO_BIN_EXE_mock_lsp_server").to_string();

    let text: atto_ui::reactive::Binding<String> = "let bad = 1;\n".to_string().into();
    let cfg = EditorConfig::new(text);
    cfg.language_id.set("rust".to_string());
    cfg.syntax.set(EditorSyntaxConfig::None);
    cfg.hover.enabled.set(false);
    cfg.lsp.set(EditorLspMode::Enabled(EditorLspConfig {
        command: vec![server_bin],
        document_uri: document_uri.to_string(),
        language_id: "rust".to_string(),
        root_uri: None,
        workspace_folders: Vec::new(),
        initialize_timeout: Duration::from_secs(1),
        semantic_tokens: false,
        folding_ranges: false,
    }));

    let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let (mut view, handle) = EditorView::new(cfg, theme);
    let backend = ratatui::backend::TestBackend::new(80, 10);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let app_theme = atto_ui::theme::Theme::dark();
    let ctx = ComponentContext {
        theme: &app_theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let area = Rect::new(0, 0, 80, 10);

    terminal
        .draw(|f| view.draw(f, area, ctx))
        .expect("initial draw");
    let rename = Event::Key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    view.handle_event(&rename, ctx);

    let message = wait_for_lsp_message(&mut terminal, &mut view, &handle, area, ctx, expected);
    assert!(
        message.contains(expected),
        "expected rename prepare message containing {expected:?}, got {message:?}"
    );
    assert!(handle.rename_popup.get().is_none());
}

fn wait_for_signature_help_popup(
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    view: &mut EditorView,
    handle: &EditorViewHandle,
    area: Rect,
    ctx: ComponentContext<'_>,
) -> SignatureHelpPopupModel {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        if let Some(popup) = handle.signature_help_popup.get() {
            return popup;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for signature help popup");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_signature_help_popup_to_clear(
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    view: &mut EditorView,
    handle: &EditorViewHandle,
    area: Rect,
    ctx: ComponentContext<'_>,
) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        if handle.signature_help_popup.get().is_none() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for signature help popup to clear");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_code_action_popup(
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    view: &mut EditorView,
    handle: &EditorViewHandle,
    area: Rect,
    ctx: ComponentContext<'_>,
) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        if handle.code_action_popup.get().is_some() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for code action popup");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_rename_popup(
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    view: &mut EditorView,
    handle: &EditorViewHandle,
    area: Rect,
    ctx: ComponentContext<'_>,
) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        if handle.rename_popup.get().is_some() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for rename popup");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_rename_workspace_edit(
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    view: &mut EditorView,
    handle: &EditorViewHandle,
    area: Rect,
    ctx: ComponentContext<'_>,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut seen = Vec::new();
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        for event in handle.events.drain() {
            match event {
                EditorEvent::LspRenameWorkspaceEdit { edit } => return edit,
                other => seen.push(other),
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for rename workspace edit; saw {seen:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_format_finished(
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    view: &mut EditorView,
    handle: &EditorViewHandle,
    area: Rect,
    ctx: ComponentContext<'_>,
) -> (bool, bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut seen = Vec::new();
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        for event in handle.events.drain() {
            match event {
                EditorEvent::FormatFinished { success, changed } => return (success, changed),
                other => seen.push(other),
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for format completion; saw {seen:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_lsp_message(
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    view: &mut EditorView,
    handle: &EditorViewHandle,
    area: Rect,
    ctx: ComponentContext<'_>,
    expected: &str,
) -> String {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut seen = Vec::new();
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        for event in handle.events.drain() {
            match event {
                EditorEvent::LspMessage { message } if message.contains(expected) => {
                    return message;
                }
                other => seen.push(other),
            }
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for LSP message {expected:?}; saw {seen:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_lsp_message_and_format_finished(
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    view: &mut EditorView,
    handle: &EditorViewHandle,
    area: Rect,
    ctx: ComponentContext<'_>,
    expected: &str,
) -> (String, bool, bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut seen = Vec::new();
    let mut message = None;
    let mut finished = None;
    loop {
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
        for event in handle.events.drain() {
            match event {
                EditorEvent::LspMessage { message: msg } if msg.contains(expected) => {
                    message = Some(msg);
                }
                EditorEvent::FormatFinished { success, changed } => {
                    finished = Some((success, changed));
                }
                other => seen.push(other),
            }
        }

        if let (Some(message), Some((success, changed))) = (message.clone(), finished) {
            return (message, success, changed);
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for LSP message {expected:?} and format completion; saw {seen:?}"
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}
