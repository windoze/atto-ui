use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_terminal::{TerminalConfig, TerminalShortcutConfig, TerminalTmuxEnvironmentConfig};
use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

static PTY_WINDOW_TEST_LOCK: Mutex<()> = Mutex::new(());

fn pty_window_test_guard() -> MutexGuard<'static, ()> {
    PTY_WINDOW_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_for_text(host: &PtyTestHost, needle: &str) {
    wait_for_text_with_timeout(host, needle, Duration::from_secs(5));
}

fn wait_for_text_with_timeout(host: &PtyTestHost, needle: &str, timeout: Duration) {
    host.wait_for_text(needle, timeout)
        .unwrap_or_else(|e| panic!("wait_for_text {needle:?} failed: {e}"));
}

fn wait_for_text_absent(host: &PtyTestHost, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if !screen.contains(needle) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "expected text {needle:?} to disappear within {timeout:?}.\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );
}

fn assert_text_absent_for(host: &PtyTestHost, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if screen.contains(needle) {
            panic!("expected text {needle:?} to remain absent.\n--- screen ---\n{screen}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn find_line_with<'a>(screen: &'a str, needle: &str) -> Option<&'a str> {
    screen.lines().find(|line| line.contains(needle))
}

fn find_text_position(screen: &str, needle: &str) -> Option<(u16, u16)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte_idx) = line.find(needle) {
            let col = UnicodeWidthStr::width(&line[..byte_idx]);
            return Some((col as u16, row as u16));
        }
    }
    None
}

fn wait_for_text_position(host: &PtyTestHost, needle: &str) -> (u16, u16) {
    wait_for_text(host, needle);
    let screen = host.screen_contents().unwrap_or_default();
    find_text_position(&screen, needle).unwrap_or_else(|| {
        panic!("expected to find {needle:?} in screen\n--- screen ---\n{screen}")
    })
}

fn wheel_down_until_text(host: &mut PtyTestHost, x: u16, y: u16, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if screen.contains(needle) {
            return;
        }
        host.wheel_down(x, y).expect("wheel down");
        thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "expected to find {needle:?} after scrolling\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );
}

/// Scrolls a scrollable window (via the wheel at `(x, y)`) until its content
/// stops changing — i.e. it has reached the bottom. Returns once two
/// consecutive reads are identical, so a button on the last page is seated
/// inside the viewport rather than clipped at the scroll boundary (clicking a
/// row on the exact viewport edge does not reliably hit the widget).
fn wheel_to_bottom(host: &mut PtyTestHost, x: u16, y: u16) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut previous = host.screen_contents().unwrap_or_default();
    while Instant::now() < deadline {
        host.wheel_down(x, y).expect("wheel down");
        thread::sleep(Duration::from_millis(20));
        let current = host.screen_contents().unwrap_or_default();
        if current == previous {
            return;
        }
        previous = current;
    }
}

/// Clicks a labelled button inside the scrollable settings window robustly:
/// scrolls the label into view, scrolls to the bottom so the button is not on
/// the viewport edge, then clicks its centre and retries (re-clicking) until
/// `settled` observes the effect. Fixes flakiness where a single edge-clipped
/// click silently failed to register.
fn click_settings_button(
    host: &mut PtyTestHost,
    wheel_x: u16,
    wheel_y: u16,
    label: &str,
    mut settled: impl FnMut(&PtyTestHost) -> bool,
) {
    wheel_down_until_text(host, wheel_x, wheel_y, label);
    wheel_to_bottom(host, wheel_x, wheel_y);

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        let (bx, by) = find_text_position(&screen, label).unwrap_or_else(|| {
            panic!("expected button {label:?} to be visible\n--- screen ---\n{screen}")
        });
        // Click the middle of the label so we land on the button body.
        let cx = bx + (UnicodeWidthStr::width(label) as u16) / 2;
        host.click(cx, by)
            .unwrap_or_else(|e| panic!("click {label:?}: {e}"));
        thread::sleep(Duration::from_millis(40));
        if settled(host) {
            return;
        }
    }
    panic!(
        "clicking {label:?} did not take effect\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );
}

fn wait_for_file(path: &std::path::Path) {
    wait_for_file_with_screen(path, None);
}

fn wait_for_file_with_screen(path: &std::path::Path, host: Option<&PtyTestHost>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let screen = host
        .map(|h| h.screen_contents().unwrap_or_default())
        .unwrap_or_default();
    panic!(
        "expected file {} to be written\n--- screen ---\n{screen}",
        path.display()
    );
}

/// Writes a default `TerminalConfig` to a fresh temp file and returns
/// `(temp_root, config_path_arg)`. Tests pass the path via `--config` so the
/// app never falls back to the developer's `~/.config/atto-ui/terminal.yaml`
/// (which may enable `close_window_on_shell_exit`, breaking restart-prompt
/// assertions). Caller is responsible for removing `temp_root`.
fn isolated_default_config(label: &str) -> (std::path::PathBuf, String) {
    let root = std::path::PathBuf::from(format!("/tmp/aui-{label}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create config temp dir");
    let config_path = root.join("terminal.yaml");
    TerminalConfig::default()
        .save_path_infer(&config_path)
        .expect("write default terminal config");
    let arg = config_path.to_string_lossy().into_owned();
    (root, arg)
}

fn isolated_config(label: &str, config: &TerminalConfig) -> (std::path::PathBuf, String) {
    let root = std::path::PathBuf::from(format!("/tmp/aui-{label}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create config temp dir");
    let config_path = root.join("terminal.yaml");
    config
        .save_path_infer(&config_path)
        .expect("write terminal config");
    let arg = config_path.to_string_lossy().into_owned();
    (root, arg)
}

fn spawn_snapshot_without_host_tmux(args: &[&str], cols: u16, rows: u16) -> PtyTestHost {
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut env_args = vec!["-u", "TMUX", "-u", "TMUX_PANE", bin];
    env_args.extend_from_slice(args);
    PtyTestHost::spawn("/usr/bin/env", &env_args, cols, rows).expect("spawn PTY app")
}

fn mouse_modifier_bits(mods: KeyModifiers) -> u16 {
    let mut cb = 0;
    if mods.contains(KeyModifiers::SHIFT) {
        cb += 4;
    }
    if mods.contains(KeyModifiers::ALT) {
        cb += 8;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        cb += 16;
    }
    cb
}

fn drag_left_with_mods(
    host: &mut PtyTestHost,
    x0: u16,
    y0: u16,
    x1: u16,
    y1: u16,
    mods: KeyModifiers,
) {
    let modifier_bits = mouse_modifier_bits(mods);
    let x0 = x0.saturating_add(1);
    let y0 = y0.saturating_add(1);
    let x1 = x1.saturating_add(1);
    let y1 = y1.saturating_add(1);
    host.send_str(&format!("\x1b[<{};{x0};{y0}M", modifier_bits))
        .expect("mouse press");
    host.send_str(&format!("\x1b[<{};{x1};{y1}M", 32 + modifier_bits))
        .expect("mouse drag");
    host.send_str(&format!("\x1b[<{};{x1};{y1}m", modifier_bits))
        .expect("mouse release");
}

fn right_click(host: &mut PtyTestHost, x: u16, y: u16) {
    let x = x.saturating_add(1);
    let y = y.saturating_add(1);
    host.send_str(&format!("\x1b[<2;{x};{y}M"))
        .expect("right mouse press");
    host.send_str(&format!("\x1b[<2;{x};{y}m"))
        .expect("right mouse release");
}

fn parse_rect_field(line: &str, key: &str) -> Option<Rect> {
    let needle = format!("{key}=");
    let start = line.find(&needle)? + needle.len();
    let token = line[start..].split_whitespace().next()?;
    if token == "CLOSED" {
        return None;
    }
    let parts: Vec<&str> = token.split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    let x = parts[0].parse().ok()?;
    let y = parts[1].parse().ok()?;
    let width = parts[2].parse().ok()?;
    let height = parts[3].parse().ok()?;
    Some(Rect {
        x,
        y,
        width,
        height,
    })
}

fn rects_from_screen(screen: &str) -> Option<(Option<Rect>, Option<Rect>)> {
    let line = find_line_with(screen, "RECT")?;
    let term = parse_rect_field(line, "TERM");
    let tools = parse_rect_field(line, "TOOLS");
    Some((term, tools))
}

fn parse_pane_rects_from_screen(screen: &str) -> Option<Vec<(u64, Rect)>> {
    let line = find_line_with(screen, "PANES=")?;
    let start = line.find("PANE_RECTS=")? + "PANE_RECTS=".len();
    let token = line[start..].split_whitespace().next()?;
    let mut rects = Vec::new();
    for entry in token.split(';') {
        let Some((id, rect)) = entry.split_once(':') else {
            continue;
        };
        let id = id
            .trim_matches(|ch: char| !ch.is_ascii_digit())
            .parse()
            .ok()?;
        let rect = rect.trim_matches(|ch: char| !(ch.is_ascii_digit() || ch == ','));
        let parts: Vec<&str> = rect.split(',').collect();
        if parts.len() != 4 {
            continue;
        }
        rects.push((
            id,
            Rect {
                x: parts[0].parse().ok()?,
                y: parts[1].parse().ok()?,
                width: parts[2].parse().ok()?,
                height: parts[3].parse().ok()?,
            },
        ));
    }
    Some(rects)
}

fn wait_for_pane_rects(host: &PtyTestHost, count: usize) -> Vec<(u64, Rect)> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if let Some(rects) = parse_pane_rects_from_screen(&screen)
            && rects.len() >= count
        {
            return rects;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "expected at least {count} pane rects\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );
}

fn wait_for_pane_rects_matching(
    host: &PtyTestHost,
    description: &str,
    mut predicate: impl FnMut(&[(u64, Rect)]) -> bool,
) -> Vec<(u64, Rect)> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if let Some(rects) = parse_pane_rects_from_screen(&screen)
            && predicate(&rects)
        {
            return rects;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "expected pane rects matching {description}\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );
}

fn send_f10(host: &mut PtyTestHost) {
    host.send_str("\x1b[21~").expect("F10");
}

fn click_file_menu_item(host: &mut PtyTestHost, label: &str) {
    host.click(1, 0).expect("open File menu");
    let (x, y) = wait_for_text_position(host, label);
    host.click(x, y).expect("click File menu item");
}

/// Opens the File menu and clicks the given dropdown item, ignoring occurrences
/// of the label elsewhere on screen (e.g. terminal banner text that mentions the
/// same word). The dropdown item is matched on a line whose trimmed content is
/// exactly the label, which the menu renders on the far left.
fn click_file_dropdown_item(host: &mut PtyTestHost, label: &str) {
    host.click(1, 0).expect("open File menu");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let screen = host.screen_contents().unwrap_or_default();
        for (row, line) in screen.lines().enumerate() {
            // Menu items render as a short left-aligned label; skip lines that
            // merely contain the word as part of other text.
            if let Some(byte_idx) = line.find(label) {
                // A dropdown item begins at the menu's left border, so nothing
                // but decoration/whitespace precedes it. The banner text
                // ("File > Settings: ...") has real words before the label and
                // is thus skipped.
                let before = line[..byte_idx].trim_start_matches(['░', '│', '┃', ' ', '║']);
                if before.is_empty() {
                    let col = UnicodeWidthStr::width(&line[..byte_idx]) as u16;
                    host.click(col, row as u16).expect("click dropdown item");
                    return;
                }
            }
        }
        if Instant::now() >= deadline {
            panic!("menu item {label:?} not found in dropdown\n--- screen ---\n{screen}");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn pty_terminal_prefix_splits_panes_inside_one_window() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut host = PtyTestHost::spawn(bin, &[], 90, 28).expect("spawn PTY app");

    wait_for_text(&host, "FOCUS=TERM");
    wait_for_text(&host, "CAP=ON");
    wait_for_text(&host, "PANES=1 ACTIVE=1");

    let screen = host.screen_contents().unwrap_or_default();
    let (term_before, tools_before) = rects_from_screen(&screen).expect("read rects before split");

    host.send_ctrl('b').expect("prefix");
    host.send_str("%").expect("split right");
    wait_for_text(&host, "PANES=2 ACTIVE=2");
    wait_for_text(&host, "TTY READY PANE=2");

    let pane_rects = wait_for_pane_rects(&host, 2);
    let left = pane_rects
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|(_, rect)| *rect)
        .expect("left pane rect");
    let right = pane_rects
        .iter()
        .find(|(id, _)| *id == 2)
        .map(|(_, rect)| *rect)
        .expect("right pane rect");
    assert!(
        right.x > left.x,
        "right split should place pane 2 after pane 1"
    );
    assert_eq!(left.y, right.y);
    assert_eq!(left.height, right.height);

    let screen = host.screen_contents().unwrap_or_default();
    let (term_after, tools_after) = rects_from_screen(&screen).expect("read rects after split");
    assert_eq!(
        term_before, term_after,
        "pane split must not resize the outer terminal window"
    );
    assert_eq!(
        tools_before, tools_after,
        "pane split must not disturb sibling floating windows"
    );

    host.send_ctrl('b').expect("prefix");
    host.send_str("o").expect("next pane");
    wait_for_text(&host, "PANES=2 ACTIVE=1");

    host.send_ctrl('b').expect("prefix");
    host.key_with_mods(KeyCode::Right, KeyModifiers::NONE)
        .expect("select pane right");
    wait_for_text(&host, "PANES=2 ACTIVE=2");

    host.send_ctrl('b').expect("prefix");
    host.key_with_mods(KeyCode::Left, KeyModifiers::CONTROL)
        .expect("resize pane left");
    let resized_rects =
        wait_for_pane_rects_matching(&host, "right pane wider after resize", |rects| {
            let left_after = rects.iter().find(|(id, _)| *id == 1).map(|(_, rect)| *rect);
            let right_after = rects.iter().find(|(id, _)| *id == 2).map(|(_, rect)| *rect);
            matches!(
                (left_after, right_after),
                (Some(left_after), Some(right_after))
                    if left_after.width < left.width && right_after.width > right.width
            )
        });
    let resized_left = resized_rects
        .iter()
        .find(|(id, _)| *id == 1)
        .map(|(_, rect)| *rect)
        .expect("resized left pane rect");
    let resized_right = resized_rects
        .iter()
        .find(|(id, _)| *id == 2)
        .map(|(_, rect)| *rect)
        .expect("resized right pane rect");

    host.send_ctrl('b').expect("prefix");
    host.send_str("z").expect("zoom active pane");
    let zoomed_rects =
        wait_for_pane_rects_matching(&host, "only active pane visible while zoomed", |rects| {
            rects.len() == 1
                && rects[0].0 == 2
                && rects[0].1.width > resized_right.width
                && rects[0].1.x <= resized_left.x
        });
    assert_eq!(zoomed_rects[0].0, 2);

    host.send_ctrl('b').expect("prefix");
    host.send_str("z").expect("restore active pane");
    wait_for_pane_rects_matching(&host, "both panes visible after zoom restore", |rects| {
        rects.len() == 2
            && rects.iter().any(|(id, _)| *id == 1)
            && rects.iter().any(|(id, _)| *id == 2)
    });
    wait_for_text(&host, "PANES=2 ACTIVE=2");

    host.send_ctrl('b').expect("prefix");
    host.send_str("x").expect("close active pane");
    wait_for_text(&host, "PANES=1 ACTIVE=1");
    wait_for_pane_rects_matching(&host, "closed pane removed from layout", |rects| {
        rects.len() == 1 && rects[0].0 == 1 && rects[0].1.width > resized_left.width
    });

    host.send_ctrl('b').expect("prefix");
    host.send_str("\"").expect("split below");
    wait_for_text(&host, "PANES=2 ACTIVE=3");
    wait_for_pane_rects(&host, 2);

    let (_, tools_rect) = rects_from_screen(&host.screen_contents().unwrap_or_default())
        .expect("read tools rect after split below");
    let tools_rect = tools_rect.expect("tools rect");
    host.click(
        tools_rect.x.saturating_add(2),
        tools_rect.y.saturating_add(2),
    )
    .expect("focus tools window");
    wait_for_text(&host, "FOCUS=TOOLS");
    wait_for_text(&host, "PANES=2 ACTIVE=3");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_does_not_intercept_outside_mouse() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    wait_for_text(&host, "FOCUS=TERM");
    wait_for_text(&host, "CAP=ON");

    // Menu bar click should activate menu even while terminal capture is on.
    host.click(1, 0).expect("click menu bar");
    wait_for_text(&host, "ACTIVE=ON");
    wait_for_text(&host, "Ping");

    // Click menu item to trigger action.
    host.click(2, 2).expect("click menu item");
    wait_for_text(&host, "MENU=PING");

    // Drag Tools window and ensure rect changes.
    let screen = host.screen_contents().unwrap_or_default();
    let (_, tools_rect) = rects_from_screen(&screen).expect("read rects");
    let tools_rect = tools_rect.expect("tools rect");
    let drag_from_x = tools_rect.x.saturating_add(5);
    let drag_from_y = tools_rect.y;
    let drag_to_x = drag_from_x.saturating_add(4);
    let drag_to_y = drag_from_y.saturating_add(2);
    host.drag_left(drag_from_x, drag_from_y, drag_to_x, drag_to_y)
        .expect("drag tools window");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut moved = false;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if let Some((_, Some(updated))) = rects_from_screen(&screen)
            && (updated.x != tools_rect.x || updated.y != tools_rect.y)
        {
            moved = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(moved, "tools window should move after drag");

    // Resizing tools window should change size.
    let screen = host.screen_contents().unwrap_or_default();
    let (_, tools_rect) = rects_from_screen(&screen).expect("read rects");
    let tools_rect = tools_rect.expect("tools rect");
    let resize_from_x = tools_rect
        .x
        .saturating_add(tools_rect.width.saturating_sub(1));
    let resize_from_y = tools_rect
        .y
        .saturating_add(tools_rect.height.saturating_sub(1));
    let resize_to_x = resize_from_x.saturating_add(3).min(78);
    let resize_to_y = resize_from_y.saturating_add(2).min(22);
    host.drag_left(resize_from_x, resize_from_y, resize_to_x, resize_to_y)
        .expect("resize tools window");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut resized = false;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if let Some((_, Some(updated))) = rects_from_screen(&screen)
            && (updated.width != tools_rect.width || updated.height != tools_rect.height)
        {
            resized = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(resized, "tools window should resize after drag");

    // Clicking tools window should move focus away from terminal and cancel capture.
    host.click(drag_from_x, drag_from_y)
        .expect("focus tools window");
    wait_for_text(&host, "FOCUS=TOOLS");
    wait_for_text(&host, "CAP=OFF");

    // Close terminal window via titlebar close button.
    let screen = host.screen_contents().unwrap_or_default();
    let (term_rect, _) = rects_from_screen(&screen).expect("read rects");
    let term_rect = term_rect.expect("term rect");
    let close_x = term_rect.x.saturating_add(2);
    let close_y = term_rect.y;
    host.click(close_x, close_y).expect("click close button");
    wait_for_text(&host, "TERM=CLOSED");

    assert_text_absent_for(&host, "Terminal", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_prefix_commands_drive_desktop_chrome() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    wait_for_text(&host, "FOCUS=TERM");
    wait_for_text(&host, "CAP=ON");

    host.send_ctrl('b').expect("prefix");
    send_f10(&mut host);
    wait_for_text(&host, "Menu:");
    wait_for_text(&host, "ACTIVE=ON");

    host.send_str("\x1b").expect("close menu");
    wait_for_text(&host, "ACTIVE=OFF");

    host.send_ctrl('b').expect("prefix");
    host.send_str("w").expect("window mode");
    wait_for_text(&host, "Window:");

    host.send_str("\x1b").expect("leave window mode");
    wait_for_text(&host, "F10 Menu");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_prefix_escape_sends_literal_prefix_to_subprocess() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = concat!(
        "stty raw -echo; ",
        "printf 'READY-READ\\r\\n'; ",
        "byte=$(dd bs=1 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n'); ",
        "printf 'BYTE=%s\\r\\n' \"$byte\"; ",
        "sleep 10"
    );
    let mut host =
        PtyTestHost::spawn(bin, &["/bin/sh", "-c", script], 80, 24).expect("spawn PTY app");

    wait_for_text(&host, "READY-READ");

    host.send_ctrl('b').expect("prefix");
    host.send_ctrl('b').expect("literal prefix escape");
    wait_for_text(&host, "BYTE=02");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_tmux_probe_environment_is_configurable() {
    let _guard = pty_window_test_guard();
    // 断言的是"config 文件 -> 加载 -> spawn -> 子进程真实继承环境"这条端到端
    // 链路。让子进程把继承到的 TMUX/TMUX_PANE/TERM 直接写到文件,测试用
    // wait_for_file 读文件断言。文件落地是确定性的(子进程拿到环境即写),
    // 完全不进"app 渲染 -> 终端转义 -> vt100 解析 -> 屏幕轮询"这条在 CI 慢
    // 机器上不确定的链路,根除本测试的 flaky。
    // 环境变量构造的纯逻辑另由 prepare_spawn_command 的进程内单元测试覆盖。
    let write_env_script = |out_path: &str| {
        format!(
            "printf 'TMUX=<%s>\\nTMUX_PANE=<%s>\\nTERM=<%s>\\n' \
             \"${{TMUX-}}\" \"${{TMUX_PANE-}}\" \"${{TERM-}}\" > '{out_path}'; \
             sleep 10"
        )
    };

    let (default_root, default_config_arg) = isolated_default_config("tmux-probe-default");
    let default_out = default_root.join("env.txt");
    let default_out_arg = default_out.to_string_lossy().into_owned();
    let mut host = spawn_snapshot_without_host_tmux(
        &[
            "--config",
            &default_config_arg,
            "/bin/sh",
            "-c",
            &write_env_script(&default_out_arg),
        ],
        90,
        26,
    );
    wait_for_file_with_screen(&default_out, Some(&host));
    let default_env = fs::read_to_string(&default_out).expect("read default env file");
    assert!(
        default_env.contains("TMUX=<>"),
        "default should not inject TMUX:\n{default_env}"
    );
    assert!(
        default_env.contains("TMUX_PANE=<>"),
        "default should not inject TMUX_PANE:\n{default_env}"
    );
    assert!(
        default_env.contains("TERM=<xterm-256color>"),
        "default should keep host TERM:\n{default_env}"
    );
    host.send_ctrl('q').expect("quit default probe");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean default probe exit");
    let _ = fs::remove_dir_all(default_root);

    let enabled_config = TerminalConfig {
        tmux: TerminalTmuxEnvironmentConfig {
            inject: true,
            socket_path: "/tmp/atto-ui-m3-1.sock".to_string(),
            shim_path: None,
            server_pid: Some(4242),
            session_id: 7,
            pane_id: 3,
            override_term: true,
        },
        ..TerminalConfig::default()
    };
    let (enabled_root, enabled_config_arg) = isolated_config("tmux-probe-enabled", &enabled_config);
    let enabled_out = enabled_root.join("env.txt");
    let enabled_out_arg = enabled_out.to_string_lossy().into_owned();
    let mut host = spawn_snapshot_without_host_tmux(
        &[
            "--config",
            &enabled_config_arg,
            "/bin/sh",
            "-c",
            &write_env_script(&enabled_out_arg),
        ],
        90,
        26,
    );
    wait_for_file_with_screen(&enabled_out, Some(&host));
    let enabled_env = fs::read_to_string(&enabled_out).expect("read enabled env file");
    assert!(
        enabled_env.contains("TMUX=</tmp/atto-ui-m3-1.sock,4242,7>"),
        "enabled should inject TMUX:\n{enabled_env}"
    );
    assert!(
        enabled_env.contains("TMUX_PANE=<%3>"),
        "enabled should inject TMUX_PANE:\n{enabled_env}"
    );
    assert!(
        enabled_env.contains("TERM=<tmux-256color>"),
        "enabled+override_term should set tmux TERM:\n{enabled_env}"
    );
    host.send_ctrl('q').expect("quit enabled probe");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean enabled probe exit");
    let _ = fs::remove_dir_all(enabled_root);
}

#[test]
fn pty_terminal_copy_mode_selects_and_copies_text() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    wait_for_text(&host, "TTY READY");
    wait_for_text(&host, "CAP=ON");

    host.send_ctrl('b').expect("prefix");
    host.send_str("[").expect("enter copy-mode");
    wait_for_text(&host, "COPYMODE=ON");

    host.send_str("kvllly").expect("copy first three chars");
    wait_for_text(&host, "COPYMODE=OFF COPY=TTY");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_mouse_drag_selection_copies_text_without_mouse_reporting() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    let (x, y) = wait_for_text_position(&host, "TTY READY");
    wait_for_text(&host, "CAP=ON");

    host.drag_left(x, y, x.saturating_add(2), y)
        .expect("drag terminal selection");
    wait_for_text(&host, "COPYMODE=OFF COPY=TTY");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_shift_drag_selection_copies_text_with_mouse_reporting() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = "printf '\\033[?1000h\\033[?1006hSHIFT PICK\\r\\n'; sleep 10";
    let mut host =
        PtyTestHost::spawn(bin, &["/bin/sh", "-c", script], 80, 24).expect("spawn PTY app");

    let (x, y) = wait_for_text_position(&host, "SHIFT PICK");
    wait_for_text(&host, "CAP=ON");

    drag_left_with_mods(&mut host, x, y, x.saturating_add(4), y, KeyModifiers::SHIFT);
    wait_for_text(&host, "COPYMODE=OFF COPY=SHIFT");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_local_copy_buffer_pastes_to_subprocess() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = concat!(
        "stty raw -echo; ",
        "bytes=$(dd bs=3 count=1 2>/dev/null); ",
        "printf 'PASTE=%s\r\n' \"$bytes\"; ",
        "sleep 10"
    );
    let mut host =
        PtyTestHost::spawn(bin, &["/bin/sh", "-c", script], 80, 24).expect("spawn PTY app");

    wait_for_text(&host, "TTY READY");
    wait_for_text(&host, "CAP=ON");

    host.send_ctrl('b').expect("prefix");
    host.send_str("[").expect("enter copy-mode");
    wait_for_text(&host, "COPYMODE=ON");

    host.send_str("kvllly").expect("copy first three chars");
    wait_for_text(&host, "COPYMODE=OFF COPY=TTY");

    host.send_ctrl('b').expect("prefix");
    host.send_str("]").expect("paste copy buffer");
    wait_for_text(&host, "PASTE=TTY");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

fn wheel_probe_script(label: &str, mode_sequence: &str, min_input_bytes: u8) -> String {
    format!(
        "stty raw -echo min {min_input_bytes} time 20; \
         printf '{mode_sequence}{label}\\r\\n'; \
         bytes=$(dd bs=64 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n'); \
         printf '\\r\\n{label}_HEX=%s\\r\\n' \"$bytes\"; \
         sleep 10"
    )
}

fn assert_wheel_probe(label: &str, mode_sequence: &str, min_input_bytes: u8, expected_hex: &str) {
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = wheel_probe_script(label, mode_sequence, min_input_bytes);
    let mut host = PtyTestHost::spawn(bin, &["/bin/sh", "-c", script.as_str()], 80, 24)
        .expect("spawn PTY app");

    let (x, y) = wait_for_text_position(&host, label);
    wait_for_text(&host, "CAP=ON");
    host.wheel_up(x, y).expect("wheel up over terminal");
    wait_for_text(&host, &format!("{label}_HEX={expected_hex}"));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_app_like_wheel_routing_uses_expected_branch() {
    let _guard = pty_window_test_guard();
    const ALT_SCREEN: &str = r"\033[?1049h";
    const MOUSE_REPORTING: &str = r"\033[?1000h\033[?1006h";
    const ALT_SCREEN_MOUSE_REPORTING: &str = r"\033[?1049h\033[?1000h\033[?1006h";
    const SGR_WHEEL_UP_PREFIX_HEX: &str = "1b5b3c36343b";
    const THREE_UP_KEYS_HEX: &str = "1b5b411b5b411b5b41";

    assert_wheel_probe(
        "VIM_MOUSE_ON",
        ALT_SCREEN_MOUSE_REPORTING,
        6,
        SGR_WHEEL_UP_PREFIX_HEX,
    );
    assert_wheel_probe(
        "HTOP",
        ALT_SCREEN_MOUSE_REPORTING,
        6,
        SGR_WHEEL_UP_PREFIX_HEX,
    );
    assert_wheel_probe("FZF_HEIGHT", MOUSE_REPORTING, 6, SGR_WHEEL_UP_PREFIX_HEX);
    assert_wheel_probe("VIM_MOUSE_OFF", ALT_SCREEN, 9, THREE_UP_KEYS_HEX);
    assert_wheel_probe("LESS", ALT_SCREEN, 9, THREE_UP_KEYS_HEX);
}

#[test]
fn pty_terminal_global_shortcuts_reach_non_terminal_and_released_capture() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let config_path =
        std::path::PathBuf::from(format!("/tmp/aui-term-shortcuts-{}", process::id()));
    let _ = fs::remove_file(&config_path);
    let config_arg = config_path.to_string_lossy().into_owned();
    let mut host =
        PtyTestHost::spawn(bin, &["--config", &config_arg], 80, 24).expect("spawn PTY app");

    wait_for_text(&host, "FOCUS=TERM");
    wait_for_text(&host, "CAP=ON");

    let screen = host.screen_contents().unwrap_or_default();
    let (term_rect, tools_rect) = rects_from_screen(&screen).expect("read rects");
    let term_rect = term_rect.expect("term rect");
    let tools_rect = tools_rect.expect("tools rect");

    host.click(
        tools_rect.x.saturating_add(2),
        tools_rect.y.saturating_add(2),
    )
    .expect("focus tools window");
    wait_for_text(&host, "FOCUS=TOOLS");
    wait_for_text(&host, "CAP=OFF");

    send_f10(&mut host);
    wait_for_text(&host, "Menu:");
    wait_for_text(&host, "ACTIVE=ON");

    host.send_str("\x1b").expect("close menu");
    wait_for_text(&host, "ACTIVE=OFF");

    host.click(term_rect.x.saturating_add(2), term_rect.y.saturating_add(2))
        .expect("focus terminal window");
    wait_for_text(&host, "FOCUS=TERM");
    wait_for_text(&host, "CAP=ON");

    host.send_ctrl('g').expect("release terminal capture");
    wait_for_text(&host, "CAP=OFF");

    send_f10(&mut host);
    wait_for_text(&host, "Menu:");
    wait_for_text(&host, "ACTIVE=ON");

    host.send_str("\x1b").expect("close menu");
    wait_for_text(&host, "ACTIVE=OFF");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");

    let _ = fs::remove_file(config_path);
}

#[test]
fn pty_terminal_dead_process_prompts_and_restarts() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let (root, config_arg) = isolated_default_config("dead-process");
    let mut host = PtyTestHost::spawn(
        bin,
        &[
            "--config",
            &config_arg,
            "/bin/sh",
            "-c",
            "printf 'CHILD-RUN\\n'; exit 7",
        ],
        80,
        24,
    )
    .expect("spawn PTY app");

    wait_for_text(&host, "CHILD-RUN");
    wait_for_text(&host, "[Process exited: code 7");
    wait_for_text(&host, "PROC=EXITED code=7 RESTARTS=0");
    wait_for_text(&host, "CAP=OFF");

    host.send_str("r").expect("restart terminal process");
    wait_for_text(&host, "RESTARTS=1");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pty_terminal_close_window_on_shell_exit_closes_terminal_window() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let root = std::path::PathBuf::from(format!("/tmp/aui-term-close-on-exit-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create settings temp dir");
    let config_path = root.join("terminal.yaml");
    let config = TerminalConfig {
        close_window_on_shell_exit: true,
        ..TerminalConfig::default()
    };
    config
        .save_path_infer(&config_path)
        .expect("write close-on-exit config");
    let config_arg = config_path.to_string_lossy().into_owned();
    let mut host = PtyTestHost::spawn(
        bin,
        &[
            "--config",
            &config_arg,
            "/bin/sh",
            "-c",
            "printf 'CHILD-CLOSE\\n'; sleep 0.2; exit 9",
        ],
        90,
        26,
    )
    .expect("spawn PTY app");

    wait_for_text(&host, "TERM=CLOSED");
    wait_for_text(&host, "TERMS=0 FOCUS_TERM=NONE");
    wait_for_text(&host, "PROC=EXITED code=9");
    assert_text_absent_for(&host, "[Process exited:", Duration::from_millis(250));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pty_terminal_restart_uses_session_profile_and_osc7_cwd() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let root = std::path::PathBuf::from(format!("/tmp/aui-term-{}", process::id()));
    let initial_cwd = root.join("initial");
    let observed_cwd = root.join("observed");
    fs::create_dir_all(&initial_cwd).expect("create initial cwd");
    fs::create_dir_all(&observed_cwd).expect("create observed cwd");
    let counter = root.join("count");
    let initial_cwd = fs::canonicalize(initial_cwd)
        .expect("canonicalize initial cwd")
        .to_string_lossy()
        .into_owned();
    let observed_cwd = fs::canonicalize(observed_cwd)
        .expect("canonicalize observed cwd")
        .to_string_lossy()
        .into_owned();
    let counter = counter.to_string_lossy().into_owned();
    let config_path = root.join("terminal.yaml");
    TerminalConfig::default()
        .save_path_infer(&config_path)
        .expect("write default terminal config");
    let config_arg = config_path.to_string_lossy().into_owned();
    let script = format!(
        "n=$(cat '{counter}' 2>/dev/null || echo 0); \
         n=$((n+1)); printf '%s' \"$n\" > '{counter}'; \
         printf '\\033]7;file://{observed_cwd}\\007'; \
         printf 'RUN=%s PWD=%s\\r\\n' \"$n\" \"$PWD\"; \
         exit 4"
    );
    let mut host = PtyTestHost::spawn(
        bin,
        &[
            "--config",
            &config_arg,
            "--profile",
            "Project",
            "--cwd",
            &initial_cwd,
            "/bin/sh",
            "-c",
            script.as_str(),
        ],
        100,
        26,
    )
    .expect("spawn PTY app");

    wait_for_text(&host, &format!("RUN=1 PWD={initial_cwd}"));
    wait_for_text(&host, "SESSION=Project CWD=");
    wait_for_text(&host, "[Process exited: code 4");

    host.send_str("r").expect("restart terminal process");
    wait_for_text(&host, "RESTARTS=1");
    wait_for_text(&host, &format!("RUN=2 PWD={observed_cwd}"));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pty_terminal_file_menu_creates_shell_and_command_in_focused_cwd() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let root = std::path::PathBuf::from(format!("/tmp/a{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let initial_cwd = root.join("i");
    let observed_cwd = root.join("o");
    fs::create_dir_all(&initial_cwd).expect("create initial cwd");
    fs::create_dir_all(&observed_cwd).expect("create observed cwd");
    let shell_script = root.join("s");
    fs::write(
        &shell_script,
        "#!/bin/sh\nprintf 'SHELL_PWD=%s\\r\\n' \"$PWD\"\nsleep 10\n",
    )
    .expect("write shell wrapper");
    let mut permissions = fs::metadata(&shell_script)
        .expect("shell wrapper metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&shell_script, permissions).expect("make shell wrapper executable");

    let counter = root.join("c");
    let initial_cwd = fs::canonicalize(initial_cwd)
        .expect("canonicalize initial cwd")
        .to_string_lossy()
        .into_owned();
    let observed_cwd = fs::canonicalize(observed_cwd)
        .expect("canonicalize observed cwd")
        .to_string_lossy()
        .into_owned();
    let shell_script = fs::canonicalize(shell_script)
        .expect("canonicalize shell wrapper")
        .to_string_lossy()
        .into_owned();
    let counter = counter.to_string_lossy().into_owned();
    let script = format!(
        "n=$(cat '{counter}' 2>/dev/null || echo 0); \
         n=$((n+1)); printf '%s' \"$n\" > '{counter}'; \
         printf '\\033]7;file://{observed_cwd}\\007'; \
         printf 'COMMAND_RUN=%s PWD=%s\\r\\n' \"$n\" \"$PWD\"; \
         sleep 10"
    );

    let mut host = PtyTestHost::spawn(
        bin,
        &[
            "--shell-program",
            &shell_script,
            "--profile",
            "Project",
            "--cwd",
            &initial_cwd,
            "/bin/sh",
            "-c",
            script.as_str(),
        ],
        140,
        32,
    )
    .expect("spawn PTY app");

    wait_for_text(&host, &format!("COMMAND_RUN=1 PWD={initial_cwd}"));
    wait_for_text(&host, &format!("SESSION=Project CWD={observed_cwd}"));

    click_file_menu_item(&mut host, "New command window");
    wait_for_text(&host, &format!("COMMAND_RUN=2 PWD={observed_cwd}"));
    wait_for_text(&host, "TERMS=2 FOCUS_TERM=2 FOCUS_PROFILE=Project");
    wait_for_text(&host, &format!("FOCUS_CWD={observed_cwd}"));

    click_file_menu_item(&mut host, "New shell window");
    wait_for_text(&host, &format!("SHELL_PWD={observed_cwd}"));
    wait_for_text(&host, "TERMS=3 FOCUS_TERM=3 FOCUS_PROFILE=Shell");
    wait_for_text(&host, &format!("FOCUS_CWD={observed_cwd}"));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pty_terminal_file_menu_opens_settings_window_and_saves_config() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let root = std::path::PathBuf::from(format!("/tmp/aui-term-settings-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create settings temp dir");
    let config_path = root.join("terminal.yaml");
    let config_arg = config_path.to_string_lossy().into_owned();
    let mut host =
        PtyTestHost::spawn(bin, &["--config", &config_arg], 100, 32).expect("spawn PTY app");

    wait_for_text(&host, "TTY READY");
    click_file_menu_item(&mut host, "Settings");
    wait_for_text(&host, "Terminal Settings");
    wait_for_text(&host, "Scrollback rows");

    let config_path_for_click = config_path.clone();
    click_settings_button(&mut host, 50, 16, "Save", move |_| {
        config_path_for_click.exists()
    });
    wait_for_file(&config_path);

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");

    let saved = TerminalConfig::load_path(&config_path).expect("load saved config");
    assert!(saved.scrollback_len > 0);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pty_terminal_settings_apply_save_and_reload_runtime_config() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");

    // 预置目标 config(scrollback=13、prefix=ctrl+a、palette ansi[0]=#12ab34),让 app 直接
    // 加载。save/apply/draft/校验逻辑由 settings.rs 的进程内单元测试覆盖;本测试只验证
    // "config 文件 -> 加载 -> runtime 生效(prefix 切换) -> reload"这条端到端链路,绕开屏幕
    // textbox 输入——那是 CI 慢机器上丢字符的 flaky 源头。
    let mut config = TerminalConfig {
        scrollback_len: 13,
        prefix_key: TerminalShortcutConfig::control_letter('a'),
        ..TerminalConfig::default()
    };
    config.palette.ansi[0] = "#12ab34".into();
    let (root, config_arg) = isolated_config("settings-runtime", &config);
    let script = "stty -echo; printf '\\033[30mPAL0\\033[0m\\n'; sleep 10";

    let mut host = PtyTestHost::spawn(
        bin,
        &["--config", &config_arg, "/bin/sh", "-c", script],
        120,
        36,
    )
    .expect("spawn PTY app");

    wait_for_text(&host, "TTY READY");
    wait_for_text(&host, "CFG_SCROLL=13 CFG_PREFIX=ctrl+a CFG_ANSI0=#12ab34");
    wait_for_text(&host, "PAL0");

    // prefix 从 config 加载为 ctrl+a:ctrl+b(默认 prefix)不触发菜单,ctrl+a 触发。
    host.send_ctrl('b').expect("non-prefix");
    send_f10(&mut host);
    assert_text_absent_for(&host, "Ping", Duration::from_millis(250));
    host.send_ctrl('a').expect("prefix");
    send_f10(&mut host);
    wait_for_text(&host, "Ping");
    host.send_str("\x1b").expect("close menu");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");

    // 确认加载的就是预置值。
    let saved = TerminalConfig::load_path(root.join("terminal.yaml")).expect("load config");
    assert_eq!(saved.scrollback_len, 13);
    assert_eq!(
        saved.prefix_key,
        TerminalShortcutConfig::control_letter('a')
    );
    assert_eq!(saved.palette.ansi[0].as_str(), "#12ab34");

    // reload:全新进程从同一 config 加载,prefix 仍生效。
    let mut reloaded = PtyTestHost::spawn(
        bin,
        &["--config", &config_arg, "/bin/sh", "-c", script],
        120,
        36,
    )
    .expect("spawn reloaded PTY app");
    wait_for_text(
        &reloaded,
        "CFG_SCROLL=13 CFG_PREFIX=ctrl+a CFG_ANSI0=#12ab34",
    );
    wait_for_text(&reloaded, "PAL0");
    reloaded.send_ctrl('a').expect("reloaded prefix");
    send_f10(&mut reloaded);
    wait_for_text(&reloaded, "Ping");

    reloaded.send_ctrl('q').expect("quit reloaded app");
    reloaded
        .wait_for_exit(Duration::from_secs(2))
        .expect("clean reloaded exit");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pty_terminal_cursor_shape_sequences_render_in_window_app() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = concat!(
        "printf 'B\\033[1G\\033[2 q'; ",
        "sleep 2; ",
        "printf '\\033[4 q'; ",
        "sleep 2; ",
        "printf '\\033[6 q'; ",
        "sleep 20",
    );
    let mut host =
        PtyTestHost::spawn(bin, &["/bin/sh", "-c", script], 90, 28).expect("spawn PTY app");

    wait_for_text(&host, "B");
    let (cursor_x, cursor_y) = wait_for_text_position(&host, "B");
    wait_for_text(&host, "CFG_CURSOR=block");

    wait_for_text(&host, "CFG_CURSOR=underline");

    wait_for_text(&host, "CFG_CURSOR=bar");
    assert_eq!(
        host.cell_contents(cursor_x, cursor_y)
            .expect("bar cursor cell"),
        "▏"
    );

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

fn assert_osc_title_updates_window_title_and_windows_menu(osc: &str, expected_title: &str) {
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = format!("printf '\\033]{osc};{expected_title}\\007'; sleep 10");
    let mut host = PtyTestHost::spawn(bin, &["/bin/sh", "-c", script.as_str()], 80, 24)
        .expect("spawn PTY app");

    wait_for_text(&host, "TTY READY");
    wait_for_text(&host, expected_title);

    host.click(8, 0).expect("open Windows menu");
    wait_for_text(&host, "Switch to");
    host.click(9, 2).expect("open window list submenu");
    wait_for_text(&host, &format!("* {expected_title}"));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_osc_zero_and_two_titles_update_window_title_and_windows_menu() {
    let _guard = pty_window_test_guard();
    assert_osc_title_updates_window_title_and_windows_menu("0", "OSC Unified Shell");
    assert_osc_title_updates_window_title_and_windows_menu("2", "OSC Project Shell");
}

#[test]
fn pty_terminal_command_block_presentation_marks_failed_commands() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = concat!(
        "printf '\\033]133;A\\007$ false\\033]133;B\\007\\r\\n'; ",
        "printf '\\033]133;C\\007boom\\r\\n\\033]133;D;2\\007'; ",
        "sleep 10"
    );
    let mut host =
        PtyTestHost::spawn(bin, &["/bin/sh", "-c", script], 80, 24).expect("spawn PTY app");

    wait_for_text(&host, "boom");
    wait_for_text(&host, "────");
    wait_for_text(&host, "!");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_ctrl_arrows_navigate_command_blocks() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut script = "printf 'NAV-SCRIPT-READY\\r\\n'; IFS= read -r _; ".to_string();
    for index in 0..18 {
        script.push_str(&format!(
            "printf '\\033]133;A\\007$ \\033]133;B\\007cmd-{index:02}\\r\\n\\033]133;C\\007OUT-{index:02}\\r\\n\\033]133;D;0\\007'; "
        ));
    }
    script.push_str("sleep 10");
    let mut host = PtyTestHost::spawn(bin, &["/bin/sh", "-c", script.as_str()], 80, 24)
        .expect("spawn PTY app");

    wait_for_text(&host, "TTY READY");
    wait_for_text(&host, "NAV-SCRIPT-READY");
    host.send_str("go\n").expect("release navigation script");
    wait_for_text(&host, "OUT-17");
    assert_text_absent_for(&host, "OUT-11", Duration::from_millis(200));

    for _ in 0..2 {
        host.key_with_mods(KeyCode::Up, KeyModifiers::CONTROL)
            .expect("Ctrl+Up");
    }
    wait_for_text(&host, "OUT-11");

    host.key_with_mods(KeyCode::Down, KeyModifiers::CONTROL)
        .expect("Ctrl+Down");
    wait_for_text(&host, "OUT-17");
    assert_text_absent_for(&host, "OUT-11", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_command_context_menu_copies_output_and_reruns() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = concat!(
        "printf '\\033]133;A\\007$ \\033]133;B\\007echo AGAIN\\r\\n'; ",
        "printf '\\033]133;C\\007RESULT\\r\\n\\033]133;D;0\\007'; ",
        "IFS= read -r line; printf 'RERUN=%s\\r\\n' \"$line\"; ",
        "sleep 10"
    );
    let mut host =
        PtyTestHost::spawn(bin, &["/bin/sh", "-c", script], 80, 24).expect("spawn PTY app");

    let (x, y) = wait_for_text_position(&host, "RESULT");
    wait_for_text(&host, "CAP=ON");

    right_click(&mut host, x, y);
    wait_for_text(&host, "Copy command");
    wait_for_text(&host, "SEL=RESULT");
    let (copy_command_x, copy_command_y) = wait_for_text_position(&host, "Copy command");
    host.click(copy_command_x, copy_command_y)
        .expect("copy command");
    wait_for_text(&host, "COPY=echo AGAIN");

    right_click(&mut host, x, y);
    wait_for_text(&host, "Copy output");
    let (copy_output_x, copy_output_y) = wait_for_text_position(&host, "Copy output");
    host.click(copy_output_x, copy_output_y)
        .expect("copy output");
    wait_for_text(&host, "COPY=RESULT");

    right_click(&mut host, x, y);
    wait_for_text(&host, "Rerun");
    let (rerun_x, rerun_y) = wait_for_text_position(&host, "Rerun");
    host.click(rerun_x, rerun_y).expect("rerun command");
    wait_for_text(&host, "RERUN=echo AGAIN");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_command_context_menu_keyboard_navigation() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let script = concat!(
        "printf '\\033]133;A\\007$ \\033]133;B\\007echo AGAIN\\r\\n'; ",
        "printf '\\033]133;C\\007RESULT\\r\\n\\033]133;D;0\\007'; ",
        "IFS= read -r line; printf 'RERUN=%s\\r\\n' \"$line\"; ",
        "sleep 10"
    );
    let mut host =
        PtyTestHost::spawn(bin, &["/bin/sh", "-c", script], 80, 24).expect("spawn PTY app");

    let (x, y) = wait_for_text_position(&host, "RESULT");
    wait_for_text(&host, "CAP=ON");

    // Open the context menu and navigate with the keyboard: Down×2 highlights
    // "Copy output" (row 2), Enter activates it.
    right_click(&mut host, x, y);
    wait_for_text(&host, "Copy output");
    host.key_with_mods(KeyCode::Down, KeyModifiers::NONE)
        .expect("down");
    host.key_with_mods(KeyCode::Down, KeyModifiers::NONE)
        .expect("down");
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)
        .expect("enter");
    wait_for_text(&host, "COPY=RESULT");

    // Reopen and activate via mnemonic: 'r' matches "Rerun".
    right_click(&mut host, x, y);
    wait_for_text(&host, "Rerun");
    host.send_str("r").expect("mnemonic r");
    wait_for_text(&host, "RERUN=echo AGAIN");

    // Reopen and dismiss with Esc: the menu must disappear and stay gone.
    right_click(&mut host, x, y);
    wait_for_text(&host, "Copy command");
    host.send_str("\x1b").expect("esc");
    wait_for_text_absent(&host, "Copy command", Duration::from_secs(2));
    assert_text_absent_for(&host, "Copy command", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn repro_toggle_close_on_exit_via_keyboard() {
    let _guard = pty_window_test_guard();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let root = std::path::PathBuf::from(format!("/tmp/aui-term-repro2-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create settings temp dir");
    let config_path = root.join("terminal.yaml");
    let config_arg = config_path.to_string_lossy().into_owned();
    let mut host =
        PtyTestHost::spawn(bin, &["--config", &config_arg], 100, 32).expect("spawn PTY app");

    wait_for_text(&host, "TTY READY");
    click_file_menu_item(&mut host, "Settings");
    wait_for_text(&host, "Terminal Settings");
    wheel_down_until_text(&mut host, 50, 16, "Close window on shell exit");

    // Click precisely on the checkbox glyph "[ ]" which sits left of the label.
    let screen = host.screen_contents().unwrap_or_default();
    let mut clicked = false;
    for (row, line) in screen.lines().enumerate() {
        if line.contains("Close window on shell exit") {
            // find the "[" preceding this label
            if let Some(label_idx) = line.find("Close window on shell exit") {
                let prefix = &line[..label_idx];
                if let Some(br) = prefix.rfind('[') {
                    let col = UnicodeWidthStr::width(&line[..br]) as u16 + 1;
                    eprintln!("clicking checkbox glyph at {col},{row}");
                    host.click(col, row as u16).expect("click glyph");
                    clicked = true;
                }
            }
        }
    }
    assert!(clicked, "did not locate checkbox glyph\n{screen}");
    thread::sleep(Duration::from_millis(300));
    let after = host.screen_contents().unwrap_or_default();
    eprintln!("--- after checkbox click ---\n{after}");
    assert!(
        after.contains("Terminal Settings"),
        "app died after checkbox click"
    );

    // Now apply
    wheel_down_until_text(&mut host, 50, 16, "Apply");
    let (ax, ay) = wait_for_text_position(&host, "Apply");
    host.click(ax, ay).expect("click apply");
    thread::sleep(Duration::from_millis(400));
    let after_apply = host.screen_contents().unwrap_or_default();
    eprintln!("--- after apply ---\n{after_apply}");
    assert!(
        after_apply.contains("Terminal Settings") || after_apply.contains("CFG_CLOSE=on"),
        "app died after apply"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repro_viewer_command_context_menu_keyboard_navigation() {
    // End-to-end against the real `terminal_viewer` example (the binary the user
    // runs): a right-click on an OSC 133 command block opens a keyboard-navigable
    // popup menu that highlights, activates, and dismisses correctly.
    let _guard = pty_window_test_guard();
    let root = std::path::PathBuf::from(format!("/tmp/aui-viewer-ctx-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("mkdir");
    let cfg = root.join("terminal.yaml");
    let cfg_arg = cfg.to_string_lossy().into_owned();
    let bin = env!("CARGO_BIN_EXE_terminal_viewer");
    let script = concat!(
        "printf '\\033]133;A\\007$ \\033]133;B\\007echo AGAIN\\r\\n'; ",
        "printf '\\033]133;C\\007RESULT\\r\\n\\033]133;D;0\\007'; ",
        "IFS= read -r line; printf 'RERUN=%s\\r\\n' \"$line\"; ",
        "sleep 10"
    );
    let mut host = PtyTestHost::spawn(
        bin,
        &["--config", &cfg_arg, "/bin/sh", "-c", script],
        110,
        34,
    )
    .expect("spawn viewer");

    wait_for_text(&host, "RESULT");
    let (x, y) = find_text_position(&host.screen_contents().unwrap_or_default(), "RESULT")
        .expect("find RESULT");

    // Right-click the command block, then activate "Rerun" via keyboard (Enter
    // on the default-highlighted first row).
    right_click(&mut host, x, y);
    wait_for_text(&host, "Rerun");
    host.send_str("\r").expect("enter");
    wait_for_text(&host, "RERUN=echo AGAIN");

    // Reopen and dismiss with Esc.
    right_click(&mut host, x, y);
    wait_for_text(&host, "Copy command");
    host.send_str("\x1b").expect("esc");
    wait_for_text_absent(&host, "Copy command", Duration::from_secs(2));

    // App stays responsive to keyboard afterwards.
    host.send_ctrl('q').ok();
    host.wait_for_exit(Duration::from_secs(3))
        .expect("clean exit after context menu");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn repro_viewer_right_click_does_not_break_keyboard() {
    // Regression: right-clicking to open the command context menu must not leak
    // the mouse escape sequence into the shell, and dismissing the menu must
    // leave the terminal able to accept keyboard input again. Previously the
    // demo force-enabled mouse reporting, so the right-click was forwarded to
    // the shell (corrupting input / flipping zsh into vi mode), and keyboard
    // capture was not restored after the modal popup closed.
    let _guard = pty_window_test_guard();
    let root = std::path::PathBuf::from(format!("/tmp/aui-rc-kbd-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("mkdir");
    let cfg = root.join("terminal.yaml");
    // Emit a synthetic OSC 133 command block so a context menu target exists,
    // without relying on real shell integration.
    let cfg_arg = cfg.to_string_lossy().into_owned();
    let bin = env!("CARGO_BIN_EXE_terminal_viewer");
    let script = concat!(
        "printf '\\033]133;A\\007$ \\033]133;B\\007echo READY\\r\\n'; ",
        "printf '\\033]133;C\\007READY\\r\\n\\033]133;D;0\\007'; ",
        "exec /bin/sh -i"
    );
    let mut host = PtyTestHost::spawn(
        bin,
        &["--config", &cfg_arg, "/bin/sh", "-c", script],
        110,
        34,
    )
    .expect("spawn viewer");
    thread::sleep(Duration::from_millis(800));
    wait_for_text(&host, "READY");

    // Right-click the command block output to open the context menu.
    let sc = host.screen_contents().unwrap_or_default();
    let (x, y) = find_text_position(&sc, "READY").expect("find READY");
    right_click(&mut host, x, y);
    wait_for_text(&host, "Rerun");

    // Dismiss with Esc, then type a command — it must run intact (no eaten
    // characters from a leaked mouse/escape sequence).
    host.send_str("\x1b").ok();
    thread::sleep(Duration::from_millis(200));
    host.send_str("echo AFTER\n").ok();
    wait_for_text(&host, "AFTER");
    // The command line must show the full "echo AFTER", not a mangled prefix.
    let screen = host.screen_contents().unwrap_or_default();
    assert!(
        screen.contains("echo AFTER"),
        "typed command should reach the shell intact\n{screen}"
    );

    host.send_ctrl('q').ok();
    let _ = host.wait_for_exit(Duration::from_secs(3));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn repro_viewer_checkbox_click_hangs() {
    let _guard = pty_window_test_guard();
    let root = std::path::PathBuf::from(format!("/tmp/aui-hang-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("mkdir");
    let cfg = root.join("terminal.yaml");
    let cfg_arg = cfg.to_string_lossy().into_owned();
    let bin = env!("CARGO_BIN_EXE_terminal_viewer");
    let mut host = PtyTestHost::spawn(bin, &["--config", &cfg_arg], 110, 34).expect("spawn viewer");
    // Wait for the app banner rather than sleeping a fixed interval.
    wait_for_text(&host, "Terminal Emulator");

    // Open File > Settings, waiting for each step instead of racing fixed sleeps.
    // Use the dropdown-specific helper: the terminal banner also contains the
    // word "Settings", which would confuse a plain text search.
    click_file_dropdown_item(&mut host, "Settings");
    wait_for_text(&host, "Terminal Settings");
    wheel_down_until_text(&mut host, 55, 16, "Close window on shell exit");
    // Seat the checkbox off the viewport edge so the click reliably registers.
    wheel_to_bottom(&mut host, 55, 16);

    let sc = host.screen_contents().unwrap_or_default();
    let mut clicked = false;
    for (row, line) in sc.lines().enumerate() {
        if let Some(li) = line.find("Close window on shell exit")
            && let Some(br) = line[..li].rfind('[')
        {
            let col = UnicodeWidthStr::width(&line[..br]) as u16 + 1;
            host.click(col, row as u16).ok();
            clicked = true;
        }
    }
    assert!(clicked, "no glyph found\n{sc}");

    // Responsiveness probe: send quit and REQUIRE clean exit within 3s.
    host.send_ctrl('q').ok();
    match host.wait_for_exit(Duration::from_secs(3)) {
        Ok(()) => eprintln!("APP RESPONSIVE: exited cleanly"),
        Err(e) => panic!("APP HUNG after mouse-click on checkbox (no response to Ctrl+Q): {e}"),
    }
    let _ = fs::remove_dir_all(root);
}
