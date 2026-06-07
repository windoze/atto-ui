#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;

use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind, WindowManager};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use atto_editor_app::actions::AppAction;
use atto_editor_app::window::{EditorWindowCommand, EditorWindowView};

fn write_temp_file(contents: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "atto-editor-app-scrollbars-{}.txt",
        std::process::id()
    ));
    fs::write(&path, contents).expect("write temp file");
    path
}

#[test]
fn window_border_scrollbar_renders_for_editor_view() {
    // Create a file large enough to require vertical scrolling.
    let text = (0..200).map(|i| format!("line {i}\n")).collect::<String>();
    let path = write_temp_file(&text);

    let actions: EventQueue<AppAction> = EventQueue::new();
    let commands: EventQueue<EditorWindowCommand> = EventQueue::new();

    let editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet> =
        atto_ui_editor::EditorThemeSet::default().into();
    let clipboard: atto_ui::reactive::Binding<String> = String::new().into();

    let view = EditorWindowView::new(
        actions,
        commands.clone(),
        editor_theme,
        clipboard,
        atto_ui_editor::DiagnosticsSummary::default().into(),
    );

    // Open the file in the active tab.
    commands.push(EditorWindowCommand::OpenFile(path));

    let bounds = Rect::new(0, 0, 60, 16);
    let theme = Theme::dark();

    let mut wm = WindowManager::new();
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Atto Editor",
            Rect::new(0, 0, 60, 16),
            Box::new(view),
        ),
        bounds,
    );

    let backend = TestBackend::new(bounds.width, bounds.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

    let buf = terminal.backend().buffer();

    // The vertical window-border scrollbar occupies the right border line (excluding corners).
    // Its first cell in the track should be the "up arrow" glyph when arrows are enabled.
    let window_rect = wm.windows()[0].rect.get();
    let inner = wm.windows()[0].inner_rect();
    assert!(window_rect.width >= 3 && window_rect.height >= 3);
    assert!(inner.width > 0 && inner.height > 0);

    let x = window_rect.x + window_rect.width - 1;
    let y = inner.y;
    let cell = buf.cell((x, y)).expect("scrollbar cell");
    assert_ne!(
        cell.symbol(),
        "",
        "expected a non-empty symbol at the scrollbar arrow location"
    );

    // Best-effort signal: the cell should not be the default vertical border glyph.
    // (We avoid hard-coding theme-dependent symbols.)
    assert_ne!(
        cell.symbol(),
        theme.border_set(true).vertical_left,
        "expected the scrollbar arrow to overwrite the border glyph"
    );
}
