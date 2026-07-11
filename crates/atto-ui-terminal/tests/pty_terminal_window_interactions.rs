use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::PtyTestHost;
use ratatui::layout::Rect;

fn wait_for_text(host: &PtyTestHost, needle: &str) {
    host.wait_for_text(needle, Duration::from_secs(5))
        .unwrap_or_else(|e| panic!("wait_for_text {needle:?} failed: {e}"));
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

fn terminal_border_is_flush_left(screen: &str) -> bool {
    screen
        .lines()
        .any(|line| line.starts_with('╔') && line.contains(" Terminal "))
}

fn send_f10(host: &mut PtyTestHost) {
    host.send_str("\x1b[21~").expect("F10");
}

#[test]
fn pty_terminal_does_not_intercept_outside_mouse() {
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

    let before = rects_from_screen(&host.screen_contents().unwrap_or_default())
        .and_then(|(term, _)| term)
        .expect("terminal rect before maximize");
    host.send_ctrl('b').expect("prefix");
    host.send_str("z").expect("toggle maximize");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut maximized = false;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if let Some((Some(after), _)) = rects_from_screen(&screen)
            && (after.width > before.width || after.height > before.height)
        {
            maximized = true;
            break;
        }
        if terminal_border_is_flush_left(&screen) {
            maximized = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        maximized,
        "prefix+z should maximize the terminal window\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_terminal_prefix_escape_sends_literal_prefix_to_subprocess() {
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
fn pty_terminal_copy_mode_selects_and_copies_text() {
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
fn pty_terminal_local_copy_buffer_pastes_to_subprocess() {
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

#[test]
fn pty_terminal_global_shortcuts_reach_non_terminal_and_released_capture() {
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

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
}

#[test]
fn pty_terminal_dead_process_prompts_and_restarts() {
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_window_app");
    let mut host = PtyTestHost::spawn(
        bin,
        &["/bin/sh", "-c", "printf 'CHILD-RUN\\n'; exit 7"],
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
    assert_osc_title_updates_window_title_and_windows_menu("0", "OSC Unified Shell");
    assert_osc_title_updates_window_title_and_windows_menu("2", "OSC Project Shell");
}
