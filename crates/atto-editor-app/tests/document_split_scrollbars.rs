#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;

use atto_editor_app::actions::AppAction;
use atto_editor_app::window::{EditorWindowCommand, EditorWindowView};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind, WindowManager};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

fn write_temp_file(contents: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "atto-editor-app-doc-split-scrollbars-{}.txt",
        std::process::id()
    ));
    fs::write(&path, contents).expect("write temp file");
    path
}

#[test]
fn document_split_mounts_scrollbars_on_split_divider() {
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

    // Open a long file and immediately split the active editor tab.
    commands.push(EditorWindowCommand::OpenFile(path));
    commands.push(EditorWindowCommand::SplitVertical);

    let bounds = Rect::new(0, 0, 70, 16);
    let theme = Theme::dark();

    let mut wm = WindowManager::new();
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Atto Editor",
            Rect::new(0, 0, 70, 16),
            Box::new(view),
        ),
        bounds,
    );

    let backend = TestBackend::new(bounds.width, bounds.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

    let buf = terminal.backend().buffer();
    let window = &wm.windows()[0];
    let window_rect = window.rect.get();
    let inner = window.inner_rect();

    // In split mode, `DocumentTabView` disables window-border scrollbars (it draws per-pane
    // scrollbars on the split borders instead). The window right border should therefore remain
    // the normal border glyph.
    let border_x = window_rect.x + window_rect.width - 1;
    let border_y = inner.y;
    let border_cell = buf.cell((border_x, border_y)).expect("border cell");
    assert_eq!(
        border_cell.symbol(),
        theme.border_set(true).vertical_right,
        "expected window border scrollbar to be suppressed in split mode"
    );

    // The document split divider is inside the window inner rect at the midpoint (with its own
    // 1-cell divider).
    let doc_divider_x = inner.x + (inner.width.saturating_sub(1) / 2);

    let divider_cell = buf
        .cell((doc_divider_x, inner.y))
        .expect("doc divider cell");
    assert_ne!(
        divider_cell.symbol(),
        theme.border_set(false).vertical_left,
        "expected the document split scrollbar to overwrite the divider glyph"
    );
}
