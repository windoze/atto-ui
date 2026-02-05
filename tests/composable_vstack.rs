use atto_ui::composable::{
    Component, ComponentContext, LayoutParams, ScrollbarHost, Size, TabMode, Text, VStack,
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
fn composable_vstack_layout_with_spacing() {
    let mut terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let ctx = ComponentContext {
                theme: &theme,
                window_id: WindowId::default(),
                is_focused: true,
                scrollbar_host: ScrollbarHost::Component,
                tab_mode: TabMode::Cycle,
            };

            let row = LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            };
            let mut view = VStack::new()
                .child_with_layout(Text::new("Line 1"), row)
                .child_with_layout(Text::new("Line 2"), row)
                .child_with_layout(Text::new("Line 3"), row)
                .spacing(1);

            view.draw(f, Rect::new(0, 0, 20, 10), ctx);
        })
        .unwrap();

    assert!(line_as_string(&terminal, 0, 20).contains("Line 1"));
    assert!(line_as_string(&terminal, 2, 20).contains("Line 2"));
    assert!(line_as_string(&terminal, 4, 20).contains("Line 3"));
}

#[test]
fn composable_vstack_padding_moves_content_inward() {
    let mut terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let ctx = ComponentContext {
                theme: &theme,
                window_id: WindowId::default(),
                is_focused: true,
                scrollbar_host: ScrollbarHost::Component,
                tab_mode: TabMode::Cycle,
            };

            let mut view = VStack::new().child(Text::new("Padded")).padding(2);
            view.draw(f, Rect::new(0, 0, 20, 10), ctx);
        })
        .unwrap();

    assert!(
        !line_as_string(&terminal, 0, 20).contains("Padded"),
        "padding should prevent content from rendering on row 0"
    );
    assert!(
        line_as_string(&terminal, 2, 20).contains("Padded"),
        "content should render at y=2 when padding=2"
    );
}
