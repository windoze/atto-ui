#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use atto_editor_app::actions::AppAction;
use atto_editor_app::explorer_window::{ExplorerWindowCommand, ExplorerWindowView};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind, WindowManager, WindowManagerInputMode};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
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

fn explorer_fixture(root: PathBuf) -> (WindowManager, EventQueue<AppAction>, Rect, Theme) {
    let actions: EventQueue<AppAction> = EventQueue::new();
    let commands: EventQueue<ExplorerWindowCommand> = EventQueue::new();
    let view = ExplorerWindowView::new(actions.clone(), commands, vec![root]);
    let bounds = Rect::new(0, 0, 90, 28);
    let theme = Theme::dark();

    let mut wm = WindowManager::new();
    let id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Explorer",
            Rect::new(0, 0, 44, 22),
            Box::new(view),
        ),
        bounds,
    );
    wm.focus(id);
    draw(&mut wm, bounds, &theme);
    (wm, actions, bounds, theme)
}

fn draw(wm: &mut WindowManager, bounds: Rect, theme: &Theme) {
    let backend = TestBackend::new(bounds.width, bounds.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| wm.draw(f, bounds, theme)).expect("draw");
}

fn file_row(wm: &WindowManager, row_offset: u16) -> (u16, u16) {
    let inner = wm.windows()[0].inner_rect();
    (inner.x + 4, inner.y + row_offset)
}

fn dispatch_mouse(
    wm: &mut WindowManager,
    bounds: Rect,
    theme: &Theme,
    button: MouseButton,
    x: u16,
    y: u16,
) {
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(button),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    });
    let _ = wm.handle_event(&event, bounds, WindowManagerInputMode::Normal, theme);
    let _ = wm.dispatch_to_focused_view(&event, bounds, theme);
}

fn dispatch_key(wm: &mut WindowManager, bounds: Rect, theme: &Theme, code: KeyCode) {
    let event = Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
    let _ = wm.dispatch_to_focused_view(&event, bounds, theme);
}

fn dispatch_text(wm: &mut WindowManager, bounds: Rect, theme: &Theme, text: &str) {
    for ch in text.chars() {
        dispatch_key(wm, bounds, theme, KeyCode::Char(ch));
    }
}

#[test]
fn explorer_inline_rename_commits_to_filesystem() {
    let root = unique_temp_dir("explorer_inline_rename");
    fs::create_dir_all(&root).expect("create temp dir");
    let old_path = root.join("old.txt");
    let new_path = root.join("new.txt");
    fs::write(&old_path, "hello").expect("write file");

    let (mut wm, _actions, bounds, theme) = explorer_fixture(root);
    let (x, y) = file_row(&wm, 2);
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::F(2));
    dispatch_text(&mut wm, bounds, &theme, "new.txt");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);

    assert!(!old_path.exists(), "old path should be renamed away");
    assert_eq!(fs::read_to_string(new_path).expect("renamed file"), "hello");
}

#[test]
fn explorer_inline_rename_cancel_leaves_filesystem_unchanged() {
    let root = unique_temp_dir("explorer_inline_rename_cancel");
    fs::create_dir_all(&root).expect("create temp dir");
    let old_path = root.join("old.txt");
    let new_path = root.join("cancelled.txt");
    fs::write(&old_path, "hello").expect("write file");

    let (mut wm, _actions, bounds, theme) = explorer_fixture(root);
    let (x, y) = file_row(&wm, 2);
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::F(2));
    dispatch_text(&mut wm, bounds, &theme, "cancelled.txt");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Esc);

    assert!(old_path.exists(), "original file should remain");
    assert!(
        !new_path.exists(),
        "cancelled rename must not create target"
    );
}

#[test]
fn explorer_context_menu_creates_new_file_and_folder() {
    let root = unique_temp_dir("explorer_context_new");
    fs::create_dir_all(&root).expect("create temp dir");

    let (mut wm, _actions, bounds, theme) = explorer_fixture(root.clone());
    let (x, y) = file_row(&wm, 1);

    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Right, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    dispatch_text(&mut wm, bounds, &theme, "created.txt");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);

    assert!(root.join("created.txt").is_file());

    draw(&mut wm, bounds, &theme);
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Right, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Down);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    dispatch_text(&mut wm, bounds, &theme, "created_dir");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);

    assert!(root.join("created_dir").is_dir());
}

#[test]
fn explorer_context_new_file_does_not_overwrite_existing_target() {
    let root = unique_temp_dir("explorer_context_no_overwrite");
    fs::create_dir_all(&root).expect("create temp dir");
    let existing = root.join("existing.txt");
    fs::write(&existing, "original").expect("write existing file");

    let (mut wm, actions, bounds, theme) = explorer_fixture(root);
    let (x, y) = file_row(&wm, 1);
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Right, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    dispatch_text(&mut wm, bounds, &theme, "existing.txt");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);

    assert_eq!(
        fs::read_to_string(&existing).expect("existing file"),
        "original"
    );
    let messages = actions.drain();
    assert!(
        messages.iter().any(|action| matches!(
            action,
            AppAction::ShowStatusMessage(message) if message.contains("target already exists")
        )),
        "expected no-overwrite status message, got {messages:?}"
    );
}

#[test]
fn explorer_context_new_file_rejects_empty_name_and_path_separator() {
    let root = unique_temp_dir("explorer_context_invalid_name");
    fs::create_dir_all(&root).expect("create temp dir");

    let (mut wm, actions, bounds, theme) = explorer_fixture(root.clone());
    let (x, y) = file_row(&wm, 1);
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Right, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    dispatch_text(&mut wm, bounds, &theme, "nested/name.txt");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);

    assert!(!root.join("nested").exists());
    let messages = actions.drain();
    assert!(
        messages.iter().any(|action| matches!(
            action,
            AppAction::ShowStatusMessage(message) if message.contains("cannot be empty")
        )),
        "expected empty-name status message, got {messages:?}"
    );
    assert!(
        messages.iter().any(|action| matches!(
            action,
            AppAction::ShowStatusMessage(message) if message.contains("path separators")
        )),
        "expected path-separator status message, got {messages:?}"
    );
}
