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
fn pty_nested_container_click_toggles_checkbox() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("[ ] Nested checkbox", Duration::from_secs(2))
        .expect("nested checkbox visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Nested checkbox").expect("find nested checkbox");
    host.click(col as u16, row as u16)
        .expect("click nested checkbox");
    host.wait_for_text("[x] Nested checkbox", Duration::from_secs(2))
        .expect("nested checkbox toggled on");

    host.click(col as u16, row as u16)
        .expect("click nested checkbox again");
    host.wait_for_text("[ ] Nested checkbox", Duration::from_secs(2))
        .expect("nested checkbox toggled off");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
