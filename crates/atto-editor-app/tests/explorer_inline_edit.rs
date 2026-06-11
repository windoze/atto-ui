#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use atto_editor_app::actions::{AppAction, OpenTarget};
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
    // The explorer file tree is borderless, so its first content row sits at the
    // window inner top. `row_offset` is kept 1-based (row 1 = root node) for
    // readability, hence the `- 1`.
    (inner.x + 4, inner.y + row_offset.saturating_sub(1))
}

fn dispatch_mouse(
    wm: &mut WindowManager,
    bounds: Rect,
    theme: &Theme,
    button: MouseButton,
    x: u16,
    y: u16,
) {
    dispatch_mouse_kind(wm, bounds, theme, MouseEventKind::Down(button), x, y, true);
}

fn dispatch_mouse_kind(
    wm: &mut WindowManager,
    bounds: Rect,
    theme: &Theme,
    kind: MouseEventKind,
    x: u16,
    y: u16,
    dispatch_to_view: bool,
) {
    let event = Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    });
    let action = wm.handle_event(&event, bounds, WindowManagerInputMode::Normal, theme);
    if dispatch_to_view && !action.consumed {
        let _ = wm.dispatch_to_focused_view(&event, bounds, theme);
    }
}

fn dispatch_key(wm: &mut WindowManager, bounds: Rect, theme: &Theme, code: KeyCode) {
    dispatch_key_mods(wm, bounds, theme, code, KeyModifiers::NONE);
}

fn dispatch_key_mods(
    wm: &mut WindowManager,
    bounds: Rect,
    theme: &Theme,
    code: KeyCode,
    modifiers: KeyModifiers,
) {
    let event = Event::Key(KeyEvent::new(code, modifiers));
    let _ = wm.dispatch_to_focused_view(&event, bounds, theme);
}

fn dispatch_text(wm: &mut WindowManager, bounds: Rect, theme: &Theme, text: &str) {
    for ch in text.chars() {
        dispatch_key(wm, bounds, theme, KeyCode::Char(ch));
    }
}

fn assert_opened_path(actions: &EventQueue<AppAction>, expected: &Path) {
    let drained = actions.drain();
    let expected = fs::canonicalize(expected).unwrap_or_else(|_| expected.to_path_buf());
    assert!(
        drained.iter().any(|action| matches!(
            action,
            AppAction::OpenPath {
                path,
                target: OpenTarget::NewTab
            } if fs::canonicalize(path).unwrap_or_else(|_| path.clone()) == expected
        )),
        "expected OpenPath for {}, got {drained:?}",
        expected.display()
    );
}

#[test]
fn explorer_inline_rename_commits_to_filesystem() {
    let root = unique_temp_dir("explorer_inline_rename");
    fs::create_dir_all(&root).expect("create temp dir");
    let old_path = root.join("old.txt");
    let new_path = root.join("new.txt");
    fs::write(&old_path, "hello").expect("write file");

    let (mut wm, actions, bounds, theme) = explorer_fixture(root);
    let (x, y) = file_row(&wm, 2);
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::F(2));
    dispatch_text(&mut wm, bounds, &theme, "new.txt");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);

    assert!(!old_path.exists(), "old path should be renamed away");
    assert_eq!(
        fs::read_to_string(&new_path).expect("renamed file"),
        "hello"
    );
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    assert_opened_path(&actions, &new_path);
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
fn explorer_drag_move_moves_file_into_directory() {
    let root = unique_temp_dir("explorer_drag_move");
    fs::create_dir_all(root.join("dest")).expect("create destination dir");
    let source = root.join("move.txt");
    let moved = root.join("dest").join("move.txt");
    fs::write(&source, "move me").expect("write source file");

    let (mut wm, _actions, bounds, theme) = explorer_fixture(root);
    let (x, dest_y) = file_row(&wm, 2);
    let (_, source_y) = file_row(&wm, 3);

    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, source_y);
    dispatch_mouse_kind(
        &mut wm,
        bounds,
        &theme,
        MouseEventKind::Drag(MouseButton::Left),
        x + 3,
        dest_y,
        false,
    );
    dispatch_mouse_kind(
        &mut wm,
        bounds,
        &theme,
        MouseEventKind::Up(MouseButton::Left),
        x + 3,
        dest_y,
        false,
    );

    assert!(!source.exists(), "source should be moved away");
    assert_eq!(fs::read_to_string(&moved).expect("moved file"), "move me");
}

#[test]
fn explorer_clipboard_paste_rejects_existing_targets_without_overwrite() {
    let root = unique_temp_dir("explorer_clipboard_no_overwrite");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&dest_dir).expect("create destination dir");
    let source = root.join("a.txt");
    let existing = dest_dir.join("a.txt");
    fs::write(&source, "source").expect("write source");
    fs::write(&existing, "existing").expect("write existing");

    let (mut wm, actions, bounds, theme) = explorer_fixture(root);
    let (x, dest_y) = file_row(&wm, 2);
    let (_, source_y) = file_row(&wm, 3);

    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, source_y);
    dispatch_key_mods(
        &mut wm,
        bounds,
        &theme,
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    );
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, dest_y);
    dispatch_key_mods(
        &mut wm,
        bounds,
        &theme,
        KeyCode::Char('v'),
        KeyModifiers::CONTROL,
    );

    assert_eq!(fs::read_to_string(&source).expect("source file"), "source");
    assert_eq!(
        fs::read_to_string(&existing).expect("existing file"),
        "existing"
    );

    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, source_y);
    dispatch_key_mods(
        &mut wm,
        bounds,
        &theme,
        KeyCode::Char('x'),
        KeyModifiers::CONTROL,
    );
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, dest_y);
    dispatch_key_mods(
        &mut wm,
        bounds,
        &theme,
        KeyCode::Char('v'),
        KeyModifiers::CONTROL,
    );

    assert_eq!(
        fs::read_to_string(&source).expect("cut source should remain"),
        "source"
    );
    assert_eq!(
        fs::read_to_string(&existing).expect("existing file"),
        "existing"
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
fn explorer_context_menu_creates_new_file_and_folder() {
    let root = unique_temp_dir("explorer_context_new");
    fs::create_dir_all(&root).expect("create temp dir");

    let (mut wm, actions, bounds, theme) = explorer_fixture(root.clone());
    let (x, y) = file_row(&wm, 1);

    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Right, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    dispatch_text(&mut wm, bounds, &theme, "created.txt");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);

    let created_file = root.join("created.txt");
    assert!(created_file.is_file());
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    assert_opened_path(&actions, &created_file);

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

#[cfg(unix)]
#[test]
fn explorer_inline_rename_rejects_dangling_symlink_target() {
    let root = unique_temp_dir("explorer_inline_rename_dangling_symlink");
    fs::create_dir_all(&root).expect("create temp dir");
    let old_path = root.join("old.txt");
    let dangling_target = root.join("missing.txt");
    let dangling_link = root.join("dangling.txt");
    fs::write(&old_path, "hello").expect("write file");
    std::os::unix::fs::symlink(&dangling_target, &dangling_link).expect("create symlink");

    let (mut wm, actions, bounds, theme) = explorer_fixture(root);
    let (x, y) = file_row(&wm, 2);
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::F(2));
    dispatch_text(&mut wm, bounds, &theme, "dangling.txt");
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);

    assert!(old_path.exists(), "rename must leave the source in place");
    assert!(
        fs::symlink_metadata(&dangling_link)
            .expect("dangling symlink should remain")
            .file_type()
            .is_symlink(),
        "rename must not replace the dangling symlink target"
    );
    let messages = actions.drain();
    assert!(
        messages.iter().any(|action| matches!(
            action,
            AppAction::ShowStatusMessage(message) if message.contains("target already exists")
        )),
        "expected dangling-symlink no-overwrite status message, got {messages:?}"
    );
}

#[test]
fn explorer_right_click_does_not_select_or_open_target() {
    let root = unique_temp_dir("explorer_context_right_click_selection");
    fs::create_dir_all(&root).expect("create temp dir");
    let first = root.join("a.txt");
    let second = root.join("b.txt");
    fs::write(&first, "a").expect("write first file");
    fs::write(&second, "b").expect("write second file");

    let (mut wm, actions, bounds, theme) = explorer_fixture(root);
    let (x, first_y) = file_row(&wm, 2);
    let (_, second_y) = file_row(&wm, 3);
    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Left, x, first_y);
    let _ = actions.drain();

    dispatch_mouse(&mut wm, bounds, &theme, MouseButton::Right, x, second_y);
    dispatch_key(&mut wm, bounds, &theme, KeyCode::Esc);
    let right_click_actions = actions.drain();
    assert!(
        !right_click_actions
            .iter()
            .any(|action| matches!(action, AppAction::OpenPath { .. })),
        "right-click should not open a file, got {right_click_actions:?}"
    );

    dispatch_key(&mut wm, bounds, &theme, KeyCode::Enter);
    assert_opened_path(&actions, &first);
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
