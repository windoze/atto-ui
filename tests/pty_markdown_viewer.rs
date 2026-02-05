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

fn scroll_until(host: &mut PtyTestHost, needle: &str, x: u16, y: u16) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if screen.contains(needle) {
            return;
        }
        host.wheel_down(x, y).expect("wheel down");
        thread::sleep(Duration::from_millis(10));
    }

    let screen = host.screen_contents().unwrap_or_default();
    panic!("timed out scrolling for text {needle:?}.\n--- screen ---\n{screen}");
}

#[test]
fn pty_markdown_link_click_calls_callback() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host =
        PtyTestHost::spawn(bin, &["--markdown"], 80, 24).expect("spawn PTY app (markdown)");

    host.wait_for_text("# Markdown Viewer", Duration::from_secs(2))
        .expect("markdown window rendered");

    // Find the link URL (rendered as part of `[text](url)`).
    host.wait_for_text("https://example.com/docs", Duration::from_secs(2))
        .expect("link url visible");
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "https://example.com/docs").expect("find url");

    host.click(col as u16, row as u16).expect("click url");
    host.wait_for_text("Clicked: https://example.com/docs", Duration::from_secs(2))
        .expect("callback updated label");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_markdown_code_and_table_scroll_right() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host =
        PtyTestHost::spawn(bin, &["--markdown"], 80, 24).expect("spawn PTY app (markdown)");

    // Ensure the code block is visible.
    scroll_until(&mut host, "```rust", 5, 5);
    host.wait_for_text("let very_long_line", Duration::from_secs(2))
        .expect("code content visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "let very_long_line").expect("find code line");
    let x_in_code = (col as u16).saturating_add(1);

    // Scroll right within the code block until the end marker appears.
    for _ in 0..40 {
        host.wheel_right(x_in_code, row as u16).expect("wheel right");
        if host
            .screen_contents()
            .unwrap_or_default()
            .contains("CODE_END_98765")
        {
            break;
        }
    }
    host.wait_for_text("CODE_END_98765", Duration::from_secs(2))
        .expect("code block scrolled horizontally");

    // Scroll down until the table is visible, then scroll right within it.
    scroll_until(&mut host, "## Table", 5, 5);
    host.wait_for_text("TABLE_SCROLL_RIGHT_TO_SEE_END", Duration::from_secs(2))
        .expect("table content visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "TABLE_SCROLL_RIGHT_TO_SEE_END").expect("find table row");
    let x_in_table = (col as u16).saturating_add(1);

    for _ in 0..60 {
        host.wheel_right(x_in_table, row as u16).expect("wheel right");
        if host
            .screen_contents()
            .unwrap_or_default()
            .contains("TABLE_END_98765")
        {
            break;
        }
    }
    host.wait_for_text("TABLE_END_98765", Duration::from_secs(2))
        .expect("table scrolled horizontally");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
