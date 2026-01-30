use std::time::Duration;

use chatty_test_host::PtyTestHost;

fn find_text_pos(screen: &str, needle: &str) -> Option<(usize, usize)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(col) = line.find(needle) {
            return Some((row, col));
        }
    }
    None
}

#[test]
fn pty_menu_dropdown_renders_above_windows() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    // F10 opens the menu bar.
    host.send_str("\x1b[21~").expect("F10");
    host.wait_for_text("Quit", Duration::from_secs(2))
        .expect("dropdown is visible (not overwritten by window)");

    // Close menu, then quit app.
    host.send(b"\x1b").expect("esc");
    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_bracketed_paste_inserts_unicode() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    host.send_paste("你好👋").expect("send paste");
    host.wait_for_text("你好👋", Duration::from_secs(2))
        .expect("text visible in textbox");

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_window_management_moves_window_title() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    let before = host.screen_contents().expect("screen");
    let (_, col_before) = find_text_pos(&before, "Widgets").expect("find Widgets title");

    host.send_ctrl('w').expect("enter window mode");
    host.wait_for_text("Window:", Duration::from_secs(2))
        .expect("window mode visible");

    for _ in 0..5 {
        host.send_str("\x1b[C").expect("right arrow");
    }

    host.send(b"\x1b").expect("esc");
    host.wait_for_text("F10 Menu", Duration::from_secs(2))
        .expect("back to normal mode");

    let after = host.screen_contents().expect("screen");
    let (_, col_after) = find_text_pos(&after, "Widgets").expect("find Widgets title");

    assert!(
        col_after > col_before,
        "expected Widgets title to move right (before {col_before}, after {col_after})\n--- before ---\n{before}\n--- after ---\n{after}"
    );

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_mouse_click_changes_focus_status() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Focus: 1", Duration::from_secs(2))
        .expect("widgets focused");

    // Click inside the Log window title bar (window rect x=46,y=4).
    host.click(47, 4).expect("mouse click");
    host.wait_for_text("Focus: 2", Duration::from_secs(2))
        .expect("log focused");

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
