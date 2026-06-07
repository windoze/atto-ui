use atto_ui::composable::{
    Component, ComponentContext, Divider, MouseCoordinateSpace, ScrollbarHost, TabMode, Text,
};
use atto_ui::theme::Theme;
use atto_ui::wm::WindowId;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

fn line_as_string(terminal: &Terminal<TestBackend>, y: u16, width: u16) -> String {
    let buf = terminal.backend().buffer();
    let mut out = String::new();
    for x in 0..width {
        out.push_str(buf[(x, y)].symbol());
    }
    out
}

#[test]
fn composable_text_renders() {
    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let ctx = ComponentContext {
                theme: &theme,
                window_id: WindowId::default(),
                is_focused: true,
                scrollbar_host: ScrollbarHost::Component,
                tab_mode: TabMode::Cycle,
                mouse_coordinate_space: MouseCoordinateSpace::Absolute,
                drag: None,
            };

            let mut view = Text::new("Hello, World!");
            view.draw(f, Rect::new(0, 0, 20, 1), ctx);
        })
        .unwrap();

    let line0 = line_as_string(&terminal, 0, 20);
    assert!(line0.contains("Hello, World!"));
}

#[test]
fn composable_divider_horizontal_renders() {
    let mut terminal = Terminal::new(TestBackend::new(10, 1)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let ctx = ComponentContext {
                theme: &theme,
                window_id: WindowId::default(),
                is_focused: true,
                scrollbar_host: ScrollbarHost::Component,
                tab_mode: TabMode::Cycle,
                mouse_coordinate_space: MouseCoordinateSpace::Absolute,
                drag: None,
            };

            let mut view = Divider::horizontal();
            view.draw(f, Rect::new(0, 0, 10, 1), ctx);
        })
        .unwrap();

    let line0 = line_as_string(&terminal, 0, 10);
    assert_eq!(line0, "──────────");
}
