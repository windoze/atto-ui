#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use atto_editor_app::actions::{AppAction, OpenTarget};
use atto_editor_app::explorer_window::{ExplorerWindowCommand, ExplorerWindowView};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind, WindowManager, WindowManagerInputMode};
use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("atto_editor_app_{prefix}_{nanos}"))
}

#[test]
fn explorer_double_click_emits_open_action() {
    let root = unique_temp_dir("explorer_double_click_unit");
    fs::create_dir_all(&root).expect("create temp dir");
    let file_path = root.join("open_me.txt");
    fs::write(&file_path, "hello\n").expect("write file");

    let actions: EventQueue<AppAction> = EventQueue::new();
    let commands: EventQueue<ExplorerWindowCommand> = EventQueue::new();
    let view = ExplorerWindowView::new(actions.clone(), commands, vec![root]);

    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();

    let mut wm = WindowManager::new();
    let id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Explorer",
            Rect::new(0, 0, 40, 20),
            Box::new(view),
        ),
        bounds,
    );
    wm.focus(id);

    // Draw once to ensure components have a `last_area` for mouse hit-testing.
    let backend = TestBackend::new(bounds.width, bounds.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

    let inner = wm.windows()[0].inner_rect();

    // The explorer file tree is borderless, so content starts at the inner top:
    // row 0 is the root dir node, row 1 is the first child file.
    let click_x = inner.x + 4;
    let click_y = inner.y + 1;
    let ev = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click_x,
        row: click_y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    });

    // First click selects; second click should trigger a double-click open.
    for _ in 0..2 {
        let action = wm.handle_event(&ev, bounds, WindowManagerInputMode::Normal, &theme);
        assert!(
            !action.consumed,
            "expected click in window body to reach the view"
        );
        let _ = wm.dispatch_to_focused_view(&ev, bounds, &theme);
    }

    let drained = actions.drain();
    assert!(
        drained.iter().any(|a| matches!(
            a,
            AppAction::OpenPath {
                target: OpenTarget::NewTab,
                ..
            }
        )),
        "expected a NewTab open action on double click; got {drained:?}"
    );
}
