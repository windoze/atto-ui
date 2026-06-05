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
fn pty_scrolls_with_keyboard_arrows() {
    let bin = env!("CARGO_BIN_EXE_snapshot_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("008: line for scrolling", Duration::from_secs(2))
        .expect("initial content visible");

    for _ in 0..5 {
        host.send_str("\x1b[B").expect("ArrowDown");
    }
    // After scrolling by 5 lines, a line that was previously off-screen should become visible.
    host.wait_for_text("013: line for scrolling", Duration::from_secs(2))
        .expect("scrolled down enough");
    assert_text_absent_for(&host, "000: line for scrolling", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_scrolls_with_page_keys_and_home_end() {
    let bin = env!("CARGO_BIN_EXE_snapshot_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("000: line for scrolling", Duration::from_secs(2))
        .expect("initial content visible");

    // Page down (CSI 6~) should jump by one viewport.
    host.send_str("\x1b[6~").expect("PageDown");
    host.wait_for_text("009: line for scrolling", Duration::from_secs(2))
        .expect("page down applied");

    // End (CSI F) should jump to bottom.
    host.send_str("\x1b[F").expect("End");
    host.wait_for_text("079: line for scrolling", Duration::from_secs(2))
        .expect("end scrolls to bottom");

    // Home (CSI H) should jump back to the top.
    host.send_str("\x1b[H").expect("Home");
    host.wait_for_text("000: line for scrolling", Duration::from_secs(2))
        .expect("home scrolls to top");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_scrolls_with_mouse_wheel() {
    let bin = env!("CARGO_BIN_EXE_snapshot_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("008: line for scrolling", Duration::from_secs(2))
        .expect("initial content visible");

    // A stable point inside the window's content area for an 80x24 PTY:
    // window at (2,3), inner at (3,4), padding at (1,1) -> content origin at (4,5).
    host.wheel_down(4, 5).expect("wheel down");
    // Default wheel step is 3 lines, so a line beyond the initial viewport should appear.
    host.wait_for_text("011: line for scrolling", Duration::from_secs(2))
        .expect("wheel scroll moved content");
    assert_text_absent_for(&host, "000: line for scrolling", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_scrollbar_track_click_pages() {
    let bin = env!("CARGO_BIN_EXE_snapshot_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("000: line for scrolling", Duration::from_secs(2))
        .expect("initial content visible");

    // Click the vertical scrollbar track on the window's right border near its bottom.
    // For an 80x24 PTY the window is at (2,3) with width 50, so the right border is x=51.
    // The bottom-most bar cell is an arrow button; click one row above to hit the track.
    host.click(51, 14).expect("click vbar track");
    host.wait_for_text("009: line for scrolling", Duration::from_secs(2))
        .expect("track click paged down");

    // Clicking near the top (below the up arrow) should page up.
    host.click(51, 5).expect("click vbar near top");
    host.wait_for_text("000: line for scrolling", Duration::from_secs(2))
        .expect("track click paged up");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_scrollbar_thumb_drag_scrolls_to_end() {
    let bin = env!("CARGO_BIN_EXE_snapshot_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("000: line for scrolling", Duration::from_secs(2))
        .expect("initial content visible");

    // Drag the thumb on the window's right border from near the top to near the bottom.
    host.drag_left(51, 5, 51, 14).expect("drag thumb down");
    host.wait_for_text("079: line for scrolling", Duration::from_secs(2))
        .expect("dragged to bottom");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_scrollbar_arrow_buttons_scroll_by_one_line() {
    let bin = env!("CARGO_BIN_EXE_snapshot_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Scroll test:", Duration::from_secs(2))
        .expect("initial header visible");

    assert_text_absent_for(&host, "009: line for scrolling", Duration::from_millis(200));

    // Click the down-arrow at the bottom of the vertical scrollbar on the window's right border.
    // For an 80x24 PTY the window is at (2,3) with width 50 and height 14:
    // right border x = 2 + 50 - 1 = 51, bottom arrow y = 3 + 14 - 2 = 15.
    host.click(51, 15).expect("click vbar down arrow");
    host.wait_for_text("009: line for scrolling", Duration::from_secs(2))
        .expect("down arrow scrolls by one line");
    assert_text_absent_for(&host, "Scroll test:", Duration::from_millis(200));

    // Click the up-arrow at the top of the vertical scrollbar to return to the top.
    // Up arrow is the first bar cell: y = window.y + 1 = 4.
    host.click(51, 4).expect("click vbar up arrow");
    host.wait_for_text("Scroll test:", Duration::from_secs(2))
        .expect("up arrow scrolls back by one line");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_scrolls_single_child_taller_than_viewport() {
    let bin = env!("CARGO_BIN_EXE_snapshot_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &["--long-child"], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Tall child row 00 | idle", Duration::from_secs(2))
        .expect("initial tall child content visible");

    for _ in 0..5 {
        host.send_str("\x1b[B").expect("ArrowDown");
    }
    host.wait_for_text("Tall child row 22 | idle", Duration::from_secs(2))
        .expect("lower part of tall child visible after scroll");
    assert_text_absent_for(
        &host,
        "Tall child row 00 | idle",
        Duration::from_millis(200),
    );

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_clicks_partially_visible_tall_child() {
    let bin = env!("CARGO_BIN_EXE_snapshot_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &["--long-child"], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Tall child row 00 | idle", Duration::from_secs(2))
        .expect("initial tall child content visible");

    for _ in 0..5 {
        host.send_str("\x1b[B").expect("ArrowDown");
    }
    host.wait_for_text("Tall child row 05 | idle", Duration::from_secs(2))
        .expect("tall child scrolled to row 05");

    // Stable point on the first visible content row for an 80x24 PTY; after five rows of scroll
    // this corresponds to row 05 inside the tall child.
    host.click(4, 3)
        .expect("click partially visible tall child");
    host.wait_for_text("clicked row 05", Duration::from_secs(2))
        .expect("click reached the partially visible child");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
