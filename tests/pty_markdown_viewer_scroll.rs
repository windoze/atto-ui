use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::PtyTestHost;

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
fn pty_markdown_viewer_scrolls_code_blocks_and_tables() {
    let bin = env!("CARGO_BIN_EXE_snapshot_markdown_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("CODE-LINE-00", Duration::from_secs(2))
        .expect("initial code block visible");
    host.wait_for_text("▲", Duration::from_secs(2))
        .expect("embedded vertical scrollbar visible");
    host.wait_for_text("◄", Duration::from_secs(2))
        .expect("embedded horizontal scrollbar visible");

    // For an 80x24 PTY:
    // - Desktop work area starts at y=1 (menu bar consumes row 0).
    // - Window is placed at (2, 3) and has a 1-cell border, so inner rect starts at (3, 4).
    // - MarkdownViewer is the full window content, so code block starts at (3, 4).
    let code_x = 4;
    let code_y = 4;

    // Scroll inside the code block (max height is 6; content is longer).
    host.wheel_down(code_x, code_y).expect("wheel down (code)");
    host.wheel_down(code_x, code_y).expect("wheel down (code)");
    host.wait_for_text("CODE-LINE-08", Duration::from_secs(2))
        .expect("code block scrolled vertically");
    assert_text_absent_for(&host, "CODE-LINE-00", Duration::from_millis(200));

    // Bring the long horizontal line back into view, then scroll horizontally.
    host.wheel_up(code_x, code_y).expect("wheel up (code)");
    host.wait_for_text("CODE-HSCROLL-BEGIN", Duration::from_secs(2))
        .expect("long code line visible");
    assert_text_absent_for(&host, "CODE-HSCROLL-END", Duration::from_millis(200));
    for _ in 0..24 {
        host.wheel_right(code_x, code_y.saturating_add(1))
            .expect("wheel right (code)");
    }
    host.wait_for_text("CODE-HSCROLL-END", Duration::from_secs(2))
        .expect("code block scrolled horizontally");

    // Table should render with themed (box-drawing) borders instead of ASCII "+-|".
    host.wait_for_text("ROW-00", Duration::from_secs(2))
        .expect("initial table rows visible");
    host.wait_for_text("┬", Duration::from_secs(2))
        .expect("table border uses themed junction glyphs");

    let table_x = 4;
    let table_y = 12;

    // Scroll inside the table (max height is 6; content is longer).
    assert_text_absent_for(&host, "ROW-09", Duration::from_millis(200));
    for _ in 0..6 {
        host.wheel_down(table_x, table_y)
            .expect("wheel down (table)");
    }
    host.wait_for_text("ROW-09", Duration::from_secs(2))
        .expect("table scrolled vertically");
    assert_text_absent_for(&host, "ROW-00", Duration::from_millis(200));

    // Horizontal scroll should reveal the far-right end marker.
    assert_text_absent_for(&host, "TABLE-HSCROLL-END", Duration::from_millis(200));
    for _ in 0..32 {
        host.wheel_right(table_x, table_y)
            .expect("wheel right (table)");
    }
    host.wait_for_text("TABLE-HSCROLL-END", Duration::from_secs(2))
        .expect("table scrolled horizontally");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
