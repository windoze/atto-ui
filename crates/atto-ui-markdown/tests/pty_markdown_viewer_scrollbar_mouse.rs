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

fn find_cell_in_row(host: &PtyTestHost, y: u16, x0: u16, x1: u16, needle: &str) -> Option<u16> {
    for x in x0..=x1 {
        if host.cell_contents(x, y).ok().as_deref() == Some(needle) {
            return Some(x);
        }
    }
    None
}

fn find_cell_in_col(host: &PtyTestHost, x: u16, y0: u16, y1: u16, needle: &str) -> Option<u16> {
    for y in y0..=y1 {
        if host.cell_contents(x, y).ok().as_deref() == Some(needle) {
            return Some(y);
        }
    }
    None
}

#[test]
fn pty_markdown_viewer_embedded_scrollbars_support_click_and_drag() {
    let bin = env!("CARGO_BIN_EXE_snapshot_markdown_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("CODE-LINE-00", Duration::from_secs(2))
        .expect("initial code block visible");

    // Layout notes (80x24 PTY):
    // - Desktop work area starts at y=1 (menu bar consumes row 0).
    // - Window is placed at (2, 3) and has a 1-cell border, so inner rect starts at (3, 4).
    // - MarkdownViewer is the full window content.
    let inner_x = 3;
    let inner_y = 4;

    // In `snapshot_markdown_app` the MarkdownViewer is set to `wrap_width(32)` and the embedded
    // blocks are capped at height 6. Both code and table overflow in both directions:
    // - embedded vbar steals 1 column -> viewport_w = 31
    // - embedded hbar steals 1 row    -> viewport_h = 5
    let viewport_w = 31;
    let viewport_h = 5;

    // --- Code block: clickable vertical scrollbar (arrow click) ---
    let code_top_y = inner_y;
    let code_vbar_x = inner_x + viewport_w;
    let code_vbar_down_y = code_top_y + viewport_h - 1;

    assert_eq!(
        host.cell_contents(code_vbar_x, code_top_y)
            .expect("read code vbar up arrow"),
        "▲"
    );
    assert_eq!(
        host.cell_contents(code_vbar_x, code_vbar_down_y)
            .expect("read code vbar down arrow"),
        "▼"
    );

    host.click(code_vbar_x, code_vbar_down_y)
        .expect("click code vbar down arrow");
    host.wait_for_text("CODE-LINE-04", Duration::from_secs(2))
        .expect("code block scrolled via vbar click");
    assert_text_absent_for(&host, "CODE-LINE-00", Duration::from_millis(200));

    // --- Code block: draggable horizontal scrollbar (thumb drag) ---
    let code_hbar_y = code_top_y + viewport_h;
    let code_hbar_x0 = inner_x;
    let code_hbar_x1 = inner_x + viewport_w - 1;

    assert_eq!(
        host.cell_contents(code_hbar_x0, code_hbar_y)
            .expect("read code hbar left arrow"),
        "◄"
    );
    assert_text_absent_for(&host, "CODE-HSCROLL-END", Duration::from_millis(200));

    let code_thumb_x = find_cell_in_row(&host, code_hbar_y, code_hbar_x0, code_hbar_x1, "█")
        .expect("find code hbar thumb cell");
    host.drag_left(code_thumb_x, code_hbar_y, code_hbar_x1, code_hbar_y)
        .expect("drag code hbar thumb to the right");
    host.wait_for_text("CODE-HSCROLL-END", Duration::from_secs(2))
        .expect("code block scrolled horizontally via thumb drag");

    // --- Table: clickable horizontal scrollbar (track click) ---
    // Table is rendered after the code block and a blank spacer line.
    let table_top_y = inner_y + 7;
    let table_hbar_y = table_top_y + viewport_h;
    let table_hbar_x0 = inner_x;
    let table_hbar_x1 = inner_x + viewport_w - 1;

    // --- Table: draggable vertical scrollbar (thumb drag) ---
    let table_vbar_x = inner_x + viewport_w;
    let table_vbar_y0 = table_top_y;
    let table_vbar_y1 = table_top_y + viewport_h - 1;

    assert_text_absent_for(&host, "ROW-09", Duration::from_millis(200));
    let table_thumb_y = find_cell_in_col(&host, table_vbar_x, table_vbar_y0, table_vbar_y1, "█")
        .expect("find table vbar thumb cell");
    host.drag_left(table_vbar_x, table_thumb_y, table_vbar_x, table_vbar_y1)
        .expect("drag table vbar thumb down");
    host.wait_for_text("ROW-09", Duration::from_secs(2))
        .expect("table scrolled vertically via thumb drag");

    // --- Table: clickable horizontal scrollbar (track click) ---
    assert_eq!(
        host.cell_contents(table_hbar_x0, table_hbar_y)
            .expect("read table hbar left arrow"),
        "◄"
    );
    assert_text_absent_for(&host, "TABLE-HSCROLL-END", Duration::from_millis(200));

    // Click near the right end of the track (but not on the right arrow) to page-scroll.
    let table_track_inc_x = table_hbar_x1.saturating_sub(1);
    host.click(table_track_inc_x, table_hbar_y)
        .expect("click table hbar track (page right)");
    host.click(table_track_inc_x, table_hbar_y)
        .expect("click table hbar track (page right)");
    host.wait_for_text("TABLE-HSCROLL-END", Duration::from_secs(2))
        .expect("table scrolled horizontally via track click");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
