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

fn wait_for_text_absent(host: &PtyTestHost, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if !screen.contains(needle) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let snapshot = host.screen_contents().unwrap_or_default();
    panic!("timed out waiting for text {needle:?} to disappear.\n--- screen ---\n{snapshot}");
}

#[test]
fn pty_textbox_selection_clipboard_and_placeholder() {
    let bin = env!("CARGO_BIN_EXE_snapshot_widgets_overflow_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("PLACEHOLDER", Duration::from_secs(2))
        .expect("placeholder visible");
    host.wait_for_text("alpha beta gamma", Duration::from_secs(2))
        .expect("textbox text visible");

    // Double click on "beta" to select the word, then Ctrl+X to cut it.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "beta").expect("find beta");
    host.click(col as u16, row as u16).expect("click beta");
    host.click(col as u16, row as u16)
        .expect("double click beta");
    host.send_ctrl('x').expect("cut selection");
    wait_for_text_absent(&host, "beta", Duration::from_secs(2));

    // Click into the empty textbox and paste (Ctrl+V). Placeholder should disappear.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "PLACEHOLDER").expect("find placeholder");
    host.click(col as u16, row as u16)
        .expect("focus empty textbox");
    host.send_ctrl('v').expect("paste");
    host.wait_for_text("beta", Duration::from_secs(2))
        .expect("pasted beta");
    wait_for_text_absent(&host, "PLACEHOLDER", Duration::from_secs(2));

    // Keyboard-based selection: Ctrl+A then Ctrl+X should clear and bring placeholder back.
    host.send_ctrl('a').expect("select all");
    host.send_ctrl('x').expect("cut all");
    host.wait_for_text("PLACEHOLDER", Duration::from_secs(2))
        .expect("placeholder returned");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_textbox_triple_click_selects_line() {
    let bin = env!("CARGO_BIN_EXE_snapshot_widgets_overflow_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("alpha beta gamma", Duration::from_secs(2))
        .expect("textbox text visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "alpha").expect("find alpha");
    host.click(col as u16, row as u16).expect("click");
    host.click(col as u16, row as u16).expect("double click");
    host.click(col as u16, row as u16).expect("triple click");
    host.send_ctrl('x').expect("cut selection");

    wait_for_text_absent(&host, "alpha", Duration::from_secs(2));
    wait_for_text_absent(&host, "gamma", Duration::from_secs(2));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_listbox_shows_scrollbar_and_scrolls_with_wheel() {
    let bin = env!("CARGO_BIN_EXE_snapshot_widgets_overflow_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Item 00", Duration::from_secs(2))
        .expect("listbox visible");
    host.wait_for_text("█", Duration::from_secs(2))
        .expect("scrollbar thumb visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Item 00").expect("find Item 00");
    host.wheel_down(col as u16, row as u16)
        .expect("wheel down on listbox");

    host.wait_for_text("Item 03", Duration::from_secs(2))
        .expect("list scrolled");
    wait_for_text_absent(&host, "Item 00", Duration::from_secs(2));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_tableview_scrolls_rows_but_keeps_header_visible() {
    let bin = env!("CARGO_BIN_EXE_snapshot_widgets_overflow_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Key", Duration::from_secs(2))
        .expect("table header visible");
    host.wait_for_text("K00", Duration::from_secs(2))
        .expect("table rows visible");
    host.wait_for_text("█", Duration::from_secs(2))
        .expect("scrollbar thumb visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "K00").expect("find K00");
    host.wheel_down(col as u16, row as u16)
        .expect("wheel down on table");

    host.wait_for_text("K03", Duration::from_secs(2))
        .expect("table scrolled");
    wait_for_text_absent(&host, "K00", Duration::from_secs(2));
    host.wait_for_text("Key", Duration::from_secs(2))
        .expect("header still visible");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
