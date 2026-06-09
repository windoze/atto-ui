use std::time::Duration;

use atto_ui_test_host::PtyTestHost;
use unicode_width::UnicodeWidthStr;

fn find_text_pos(screen: &str, needle: &str) -> Option<(usize, usize)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte_col) = line.find(needle) {
            let col = line[..byte_col].width();
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
fn pty_menu_mnemonic_markers_are_hidden_and_activate_item() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    host.send_str("\x1b[21~").expect("F10");
    host.wait_for_text("Quit", Duration::from_secs(2))
        .expect("dropdown is visible");

    let screen = host.screen_contents().expect("screen");
    assert!(screen.contains("File"), "screen was:\n{screen}");
    assert!(!screen.contains("&File"), "screen was:\n{screen}");

    host.send_str("q").expect("mnemonic quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("mnemonic activated quit");
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
fn pty_desktop_background_uses_texture() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    assert_eq!(
        host.cell_contents(0, 1).expect("desktop background cell"),
        "░",
        "expected textured desktop background\n--- screen ---\n{screen}"
    );

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_window_title_is_centered_with_padding() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (title_y, title_start) = find_text_pos(&screen, "Widgets").expect("Widgets title");
    assert_eq!(
        host.cell_contents((title_start - 1) as u16, title_y as u16)
            .expect("leading title padding"),
        " ",
        "expected leading title padding\n--- screen ---\n{screen}"
    );
    for (offset, ch) in "Widgets".chars().enumerate() {
        assert_eq!(
            host.cell_contents((title_start + offset) as u16, title_y as u16)
                .expect("title cell"),
            ch.to_string(),
            "expected centered Widgets title\n--- screen ---\n{screen}"
        );
    }
    assert_eq!(
        host.cell_contents((title_start + "Widgets".len()) as u16, title_y as u16)
            .expect("trailing title padding"),
        " ",
        "expected trailing title padding\n--- screen ---\n{screen}"
    );

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

    // Click inside the Log window title bar, past the left close control.
    host.click(53, 4).expect("mouse click");
    host.wait_for_text("Focus: 2", Duration::from_secs(2))
        .expect("log focused");

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
