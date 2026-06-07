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
