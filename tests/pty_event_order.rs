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
fn pty_nested_container_event_order_is_capture_target_bubble() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--event-order"], 80, 24).expect("spawn PTY app");

    host.wait_for_text("Target leaf", Duration::from_secs(2))
        .expect("target visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Target leaf").expect("find target");
    host.click(col as u16, row as u16).expect("click target");

    host.wait_for_text(
        "TRACE: root-capture>child-capture>target-handle>child-bubble>root-bubble",
        Duration::from_secs(2),
    )
    .expect("event order visible");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
