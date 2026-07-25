//! End-to-end PTY tests against the real `atm` binary (the multiplexer the
//! user runs). These were lifted out of `atto-ui-terminal` when the viewer was
//! promoted into its own crate; they spawn the `atm` binary and exercise the
//! right-click command context menu, the settings checkbox, and the keyboard
//! capture path. Helpers here are duplicated from the terminal crate's test
//! suite on purpose — they are small, test-only, and keep this crate's tests
//! self-contained.

use std::fs;
use std::process;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::PtyTestHost;
use unicode_width::UnicodeWidthStr;

static PTY_WINDOW_TEST_LOCK: Mutex<()> = Mutex::new(());

fn pty_window_test_guard() -> MutexGuard<'static, ()> {
    PTY_WINDOW_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_for_text(host: &PtyTestHost, needle: &str) {
    host.wait_for_text(needle, Duration::from_secs(5))
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

fn find_text_position(screen: &str, needle: &str) -> Option<(u16, u16)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte_idx) = line.find(needle) {
            let col = UnicodeWidthStr::width(&line[..byte_idx]);
            return Some((col as u16, row as u16));
        }
    }
    None
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

fn right_click(host: &mut PtyTestHost, x: u16, y: u16) {
    let x = x.saturating_add(1);
    let y = y.saturating_add(1);
    host.send_str(&format!("\x1b[<2;{x};{y}M"))
        .expect("right mouse press");
    host.send_str(&format!("\x1b[<2;{x};{y}m"))
        .expect("right mouse release");
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
fn repro_viewer_command_context_menu_keyboard_navigation() {
    // End-to-end against the real `atm` binary (the binary the user runs): a
    // right-click on an OSC 133 command block opens a keyboard-navigable popup
    // menu that highlights, activates, and dismisses correctly.
    let _guard = pty_window_test_guard();
    let root = std::path::PathBuf::from(format!("/tmp/aui-viewer-ctx-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("mkdir");
    let cfg = root.join("terminal.yaml");
    let cfg_arg = cfg.to_string_lossy().into_owned();
    let bin = env!("CARGO_BIN_EXE_atm");
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
    let bin = env!("CARGO_BIN_EXE_atm");
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
    let bin = env!("CARGO_BIN_EXE_atm");
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
