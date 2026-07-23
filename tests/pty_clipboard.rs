use std::time::Duration;

use atto_ui::clipboard::osc52_sequence;
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
fn pty_selectable_text_drag_copy_emits_osc52() {
    let bin = env!("CARGO_BIN_EXE_snapshot_clipboard_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("alpha beta", Duration::from_secs(2))
        .expect("selectable text visible");
    host.wait_for_text("gamma delta", Duration::from_secs(2))
        .expect("second selectable line visible");

    let screen = host.screen_contents().expect("screen");
    let (beta_row, beta_col) = find_text_pos(&screen, "beta").expect("find beta");
    let (gamma_row, gamma_col) = find_text_pos(&screen, "gamma").expect("find gamma");

    host.drag_left(
        beta_col as u16,
        beta_row as u16,
        (gamma_col + "gamma".len()) as u16,
        gamma_row as u16,
    )
    .expect("drag selectable text range");

    host.send_ctrl('c').expect("copy selection");
    let expected = osc52_sequence("beta\ngamma");
    host.wait_for_output(expected.as_bytes(), Duration::from_secs(2))
        .expect("OSC52 copy emitted");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
