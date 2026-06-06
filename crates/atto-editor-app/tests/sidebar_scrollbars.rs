#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;

use atto_editor_app::actions::AppAction;
use atto_editor_app::explorer_window::{ExplorerWindowCommand, ExplorerWindowView};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind, WindowManager};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

fn make_temp_workspace(file_count: usize) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "atto-editor-app-sidebar-scrollbars-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    for i in 0..file_count {
        let name = format!("file_{i:04}.txt");
        fs::write(dir.join(name), "x\n").expect("write temp file");
    }

    dir
}

#[test]
fn explorer_window_scrollbar_is_mounted_on_window_border() {
    let workspace = make_temp_workspace(200);

    let actions: EventQueue<AppAction> = EventQueue::new();
    let commands: EventQueue<ExplorerWindowCommand> = EventQueue::new();

    let view = ExplorerWindowView::new(actions, commands, vec![workspace]);

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

    let inner = wm.windows()[0].inner_rect();
    let buf = terminal.backend().buffer();

    let window_rect = wm.windows()[0].rect.get();

    // The vertical window-border scrollbar occupies the right border line (excluding corners).
    // Its first cell in the track should overwrite the window border glyph.
    let border_x = window_rect.x + window_rect.width - 1;
    let y = inner.y;
    let cell = buf.cell((border_x, y)).expect("scrollbar cell");
    assert_ne!(
        cell.symbol(),
        theme.border_set(true).vertical_right,
        "expected the explorer scrollbar to overwrite the window border glyph"
    );

    // The FileTree widget draws its own border inside the window inner rect; that border should
    // remain intact (the scrollbar should be on the window border, not inside the widget).
    let tree_right_border_x = inner.x + inner.width - 1;
    let tree_border_y = inner.y + 1;
    let tree_border_cell = buf
        .cell((tree_right_border_x, tree_border_y))
        .expect("file tree border cell");
    assert_eq!(
        tree_border_cell.symbol(),
        theme.border_set(false).vertical_right,
        "expected the file tree border to remain intact (scrollbar should be on the window border)"
    );
}
