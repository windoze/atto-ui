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

fn assert_text_absent_for(host: &PtyTestHost, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if screen.contains(needle) {
            panic!("expected text {needle:?} to remain absent.\n--- screen ---\n{screen}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn pty_editor_tab_inserts_to_next_tab_stop() {
    let bin = env!("CARGO_BIN_EXE_snapshot_editor_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY editor app");
    host.wait_for_text("tab:ab", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "tab:ab").expect("find tab line");

    // Click after the `b` to place the caret at the end of the line.
    let end_x = col + "tab:ab".chars().count();
    host.click(end_x as u16, row as u16)
        .expect("click line end");

    host.send(b"\t").expect("tab");
    host.send_str("X").expect("insert X");

    // With tab_width=4 and insert_spaces=true, column 6 should advance to column 8 (2 spaces).
    host.wait_for_text("tab:ab  X", Duration::from_secs(2))
        .expect("tab inserted to next stop");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_editor_ctrl_slash_toggles_rust_line_comment() {
    let bin = env!("CARGO_BIN_EXE_snapshot_editor_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY editor app");
    host.wait_for_text("let answer = 42;", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "let answer = 42;").expect("find rust line");
    host.click(col as u16, row as u16).expect("click rust line");

    // Ctrl+/ is encoded as C0 US (0x1f) by common terminals.
    host.send(&[0x1f]).expect("Ctrl+/ toggle comment");

    host.wait_for_text("// let answer = 42;", Duration::from_secs(2))
        .expect("line comment toggled on");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_editor_double_click_selects_word_and_replaces_on_type() {
    let bin = env!("CARGO_BIN_EXE_snapshot_editor_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY editor app");
    host.wait_for_text("double: world", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "world").expect("find word");

    host.click(col as u16, row as u16).expect("click 1");
    host.click(col as u16, row as u16).expect("click 2");
    host.send_str("X").expect("type over selection");

    host.wait_for_text("double: X", Duration::from_secs(2))
        .expect("word replaced");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_editor_triple_click_selects_line_and_replaces_on_type() {
    let bin = env!("CARGO_BIN_EXE_snapshot_editor_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY editor app");
    host.wait_for_text("triple: full-line", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "triple:").expect("find line");

    host.click(col as u16, row as u16).expect("click 1");
    host.click(col as u16, row as u16).expect("click 2");
    host.click(col as u16, row as u16).expect("click 3");
    host.send_str("ZZZ").expect("type over line selection");

    host.wait_for_text("ZZZ", Duration::from_secs(2))
        .expect("line replaced");
    assert_text_absent_for(&host, "triple:", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_editor_rectangular_selection_inserts_on_multiple_lines() {
    let bin = env!("CARGO_BIN_EXE_snapshot_editor_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY editor app");
    host.wait_for_text("rect:ab", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "rect:ab").expect("find rect block");

    // Toggle rectangle selection mode (Ctrl+B), then drag over the "ab" columns across 3 lines.
    host.send_ctrl('b').expect("toggle rect selection");

    let start_x = col + "rect:".chars().count();
    let end_x = start_x + 2;

    host.drag_left(start_x as u16, row as u16, end_x as u16, (row + 2) as u16)
        .expect("drag rect selection");
    host.send_str("X").expect("type over rect selection");

    host.wait_for_text("rect:X", Duration::from_secs(2))
        .expect("rect selection replaced");
    assert_text_absent_for(&host, "rect:ab", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_editor_rectangular_selection_works_with_keyboard() {
    let bin = env!("CARGO_BIN_EXE_snapshot_editor_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY editor app");
    host.wait_for_text("rect:ab", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "rect:ab").expect("find rect block");

    // Place caret just before the "ab".
    let start_x = col + "rect:".chars().count();
    host.click(start_x as u16, row as u16)
        .expect("click rect start");

    // Toggle rectangle selection mode (Ctrl+B), then use Shift+Arrow keys to expand the block
    // selection (2 columns wide, 3 lines tall).
    host.send_ctrl('b').expect("toggle rect selection");
    host.send_str("\u{1b}[1;2C").expect("Shift+Right");
    host.send_str("\u{1b}[1;2C").expect("Shift+Right");
    host.send_str("\u{1b}[1;2B").expect("Shift+Down");
    host.send_str("\u{1b}[1;2B").expect("Shift+Down");

    host.send_str("X").expect("type over rect selection");

    host.wait_for_text("rect:X", Duration::from_secs(2))
        .expect("rect selection replaced");
    assert_text_absent_for(&host, "rect:ab", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_editor_page_down_reaches_end_of_document() {
    let bin = env!("CARGO_BIN_EXE_snapshot_editor_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY editor app");
    host.wait_for_text("tab:ab", Duration::from_secs(2))
        .expect("initial render");

    // Page down enough times to reach EOF. Historically this could get stuck one "page" short
    // when the cursor move overshot the last line.
    for _ in 0..80 {
        host.send_str("\u{1b}[6~").expect("PageDown");
    }

    host.wait_for_text("pd:119 line for paging", Duration::from_secs(2))
        .expect("paged down to bottom");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_editor_escape_clears_selection_when_no_popups() {
    let bin = env!("CARGO_BIN_EXE_snapshot_editor_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY editor app");
    host.wait_for_text("double: world", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "world").expect("find word");

    // Double click to select "world", then Esc should clear the selection.
    host.click(col as u16, row as u16).expect("click 1");
    host.click(col as u16, row as u16).expect("click 2");
    host.send_str("\u{1b}").expect("Esc");
    // Give the terminal parser time to disambiguate bare Esc from an Alt-modified key.
    thread::sleep(Duration::from_millis(100));
    host.send_str("X").expect("type");

    host.wait_for_text("double: worldX", Duration::from_secs(2))
        .expect("selection cleared before typing");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
