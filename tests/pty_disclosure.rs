use std::time::Duration;

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};
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
fn pty_disclosure_toggles_status_and_streamed_content() {
    let bin = env!("CARGO_BIN_EXE_snapshot_disclosure_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("▶ [~] Tool Call", Duration::from_secs(2))
        .expect("collapsed running disclosure visible");
    let screen = host.screen_contents().expect("screen");
    assert!(
        !screen.contains("chunk 1 ready"),
        "collapsed disclosure should hide content.\n--- screen ---\n{screen}"
    );

    host.key_with_mods(KeyCode::Enter, KeyModifiers::empty())
        .expect("expand with Enter");
    host.wait_for_text("▼ [~] Tool Call", Duration::from_secs(2))
        .expect("expanded running disclosure visible");
    host.wait_for_text("chunk 1 ready", Duration::from_secs(2))
        .expect("bound content visible after expand");

    host.send_str("a").expect("append content");
    host.wait_for_text("chunk 2 appended", Duration::from_secs(2))
        .expect("streamed content append visible");

    host.send_str("d").expect("set done status");
    host.wait_for_text("▼ [x] Tool Call", Duration::from_secs(2))
        .expect("done status visible");
    host.send_str("e").expect("set error status");
    host.wait_for_text("▼ [!] Tool Call", Duration::from_secs(2))
        .expect("error status visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Tool Call").expect("find disclosure title");
    host.click(col as u16, row as u16)
        .expect("collapse with mouse");
    host.wait_for_screen(
        |rows| {
            let screen = rows.join("\n");
            screen.contains("▶ [!] Tool Call") && !screen.contains("chunk 1 ready")
        },
        Duration::from_secs(2),
    )
    .expect("mouse click collapses and hides content");

    host.click(col as u16, row as u16)
        .expect("expand with mouse");
    host.wait_for_text("chunk 2 appended", Duration::from_secs(2))
        .expect("mouse click expands content again");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
