use std::thread;
use std::time::{Duration, Instant};

use chatty_test_host::PtyTestHost;

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

    // Click the vertical scrollbar track near its bottom.
    host.click(50, 15).expect("click vbar track");
    host.wait_for_text("009: line for scrolling", Duration::from_secs(2))
        .expect("track click paged down");

    // Clicking near the top should page up.
    host.click(50, 4).expect("click vbar near top");
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

    // Drag the thumb from the top of the track to the bottom.
    host.drag_left(50, 4, 50, 15).expect("drag thumb down");
    host.wait_for_text("079: line for scrolling", Duration::from_secs(2))
        .expect("dragged to bottom");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
