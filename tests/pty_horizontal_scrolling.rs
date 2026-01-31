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
fn pty_horizontal_scrolls_with_home_end() {
    let bin = env!("CARGO_BIN_EXE_snapshot_hscroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("[col-00]", Duration::from_secs(2))
        .expect("initial content visible");

    // End (CSI F) should jump to the far right.
    host.send_str("\x1b[F").expect("End");
    host.wait_for_text("[col-39]", Duration::from_secs(2))
        .expect("end scrolls to the rightmost content");

    // Home (CSI H) should jump back to the start.
    host.send_str("\x1b[H").expect("Home");
    host.wait_for_text("[col-00]", Duration::from_secs(2))
        .expect("home scrolls back to the leftmost content");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_horizontal_scrollbar_thumb_drag_scrolls_to_end() {
    let bin = env!("CARGO_BIN_EXE_snapshot_hscroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("[col-00]", Duration::from_secs(2))
        .expect("initial content visible");

    // For an 80x24 PTY:
    // - Desktop work area starts at y=1 (menu bar consumes row 0).
    // - Window is placed at (2, 3) and has a 1-cell border, so inner rect starts at (3, 4).
    // - With a window height of 8, the inner height is 6, and the horizontal scrollbar (thickness=1)
    //   is on the bottom border row: y = window.y + window.height - 1 = 3 + 8 - 1 = 10.
    // - Arrow buttons occupy the ends of the bar, so start the drag one cell in from the left arrow.
    host.drag_left(4, 10, 50, 10).expect("drag thumb right");

    host.wait_for_text("[col-39]", Duration::from_secs(2))
        .expect("dragged to the far right");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_horizontal_scrollbar_arrow_button_scrolls_by_one_column() {
    let bin = env!("CARGO_BIN_EXE_snapshot_hscroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("[col-00]", Duration::from_secs(2))
        .expect("initial content visible");

    // `[col-05]` is not fully visible at the initial scroll position.
    assert_text_absent_for(&host, "[col-05]", Duration::from_millis(200));

    // Click the right arrow on the horizontal scrollbar (bottom border, one cell left of the
    // bottom-right corner).
    for _ in 0..10 {
        host.click(50, 10).expect("click hbar right arrow");
    }

    host.wait_for_text("[col-05]", Duration::from_secs(2))
        .expect("right arrow scrolls by small increments");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
