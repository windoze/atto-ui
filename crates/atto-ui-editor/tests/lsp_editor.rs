use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::style::Color;

use atto_ui::composable::{Component, ComponentContext, EventHandling, ScrollbarHost, TabMode};
use atto_ui::wm::WindowId;
use atto_ui_editor::{EditorConfig, EditorLspConfig, EditorLspMode, EditorSyntaxConfig};
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
