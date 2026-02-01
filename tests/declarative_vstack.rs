use chatty::declarative::{DeclarativeView, Text, VStack};
use chatty::theme::Theme;
use chatty::view::{ScrollbarHost, ViewContext};
use chatty::wm::WindowId;
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
fn declarative_vstack_layout_with_spacing() {
    let mut terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let ctx = ViewContext {
                theme: &theme,
                window_id: WindowId::default(),
                is_focused: true,
                scrollbar_host: ScrollbarHost::default(),
            };

            let view = VStack::new()
                .child(Text::new("Line 1"))
                .child(Text::new("Line 2"))
                .child(Text::new("Line 3"))
                .spacing(1);

            view.render(f, Rect::new(0, 0, 20, 10), ctx);
        })
        .unwrap();

    assert!(line_as_string(&terminal, 0, 20).contains("Line 1"));
    assert!(line_as_string(&terminal, 2, 20).contains("Line 2"));
    assert!(line_as_string(&terminal, 4, 20).contains("Line 3"));
}

#[test]
fn declarative_vstack_padding_moves_content_inward() {
    let mut terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let ctx = ViewContext {
                theme: &theme,
                window_id: WindowId::default(),
                is_focused: true,
                scrollbar_host: ScrollbarHost::default(),
            };

            let view = VStack::new().child(Text::new("Padded")).padding(2);
            view.render(f, Rect::new(0, 0, 20, 10), ctx);
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
