use std::time::Duration;

use chatty_test_host::PtyTestHost;

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
    //   is on the bottom row of the inner rect: y = 4 + (6 - 1) = 9.
    host.drag_left(3, 9, 50, 9).expect("drag thumb right");

    host.wait_for_text("[col-39]", Duration::from_secs(2))
        .expect("dragged to the far right");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
