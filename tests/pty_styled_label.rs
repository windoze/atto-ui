use std::time::Duration;

use atto_ui_test_host::PtyTestHost;
use unicode_width::UnicodeWidthStr;

fn find_text_pos(screen: &str, needle: &str) -> Option<(usize, usize)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte_idx) = line.find(needle) {
            let col = UnicodeWidthStr::width(&line[..byte_idx]);
            return Some((row, col));
        }
    }
    None
}

#[test]
fn pty_styled_label_strips_markers_and_handles_link_click() {
    let bin = env!("CARGO_BIN_EXE_snapshot_styled_label_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("BOLD ITALIC UNDER STRIKE LINK", Duration::from_secs(2))
        .expect("styled label visible");

    let screen = host.screen_contents().expect("screen");
    assert!(
        !screen.contains("**BOLD**"),
        "expected bold markers to be hidden.\n--- screen ---\n{screen}"
    );
    assert!(
        !screen.contains("*ITALIC*"),
        "expected italic markers to be hidden.\n--- screen ---\n{screen}"
    );
    assert!(
        !screen.contains("__UNDER__"),
        "expected underline markers to be hidden.\n--- screen ---\n{screen}"
    );
    assert!(
        !screen.contains("~~STRIKE~~"),
        "expected strikethrough markers to be hidden.\n--- screen ---\n{screen}"
    );
    assert!(
        !screen.contains("https://example.com"),
        "expected URL to be hidden until clicked.\n--- screen ---\n{screen}"
    );

    let (row, col) = find_text_pos(&screen, "LINK").expect("find LINK text");
    host.click(col as u16, row as u16).expect("click LINK");
    host.wait_for_text("Clicked: https://example.com", Duration::from_secs(2))
        .expect("link callback fired");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
