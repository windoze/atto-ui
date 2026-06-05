use std::thread;
use std::time::{Duration, Instant};

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

fn textbox_origin(host: &PtyTestHost) -> (u16, u16) {
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "┌Text").expect("find textbox border");
    (col as u16 + 1, row as u16 + 1)
}

fn wait_for_textbox_line(host: &PtyTestHost, row: u16, expected: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if let Some(line) = screen.lines().nth(row as usize)
            && line.contains(expected)
        {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let screen = host.screen_contents().unwrap_or_default();
    panic!("timed out waiting for textbox line fragment {expected:?}.\n--- screen ---\n{screen}");
}

#[test]
fn pty_textbox_shift_click_cjk_half_deletes_grapheme_aligned_selection() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--textbox-unicode"], 80, 24).expect("spawn PTY app");
    host.wait_for_text("a你b好c", Duration::from_secs(2))
        .expect("textbox unicode fixture visible");

    let (text_x, text_y) = textbox_origin(&host);
    host.click(text_x + 7, text_y)
        .expect("place cursor after unicode text");
    host.shift_click(text_x + 2, text_y)
        .expect("shift-click inside wide character");
    host.send_str("\x1b[3~").expect("Delete");

    wait_for_textbox_line(&host, text_y, "│a ", Duration::from_secs(2));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_textbox_drag_across_cjk_deletes_complete_graphemes() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--textbox-unicode"], 80, 24).expect("spawn PTY app");
    host.wait_for_text("a你b好c", Duration::from_secs(2))
        .expect("textbox unicode fixture visible");

    let (text_x, text_y) = textbox_origin(&host);
    host.drag_left(text_x + 1, text_y, text_x + 6, text_y)
        .expect("drag selection across wide characters");
    host.send_str("\x1b[3~").expect("Delete");

    wait_for_textbox_line(&host, text_y, "│ac ", Duration::from_secs(2));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
