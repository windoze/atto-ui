use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::PtyTestHost;

static VIRTUAL_SCROLL_PTY_LOCK: Mutex<()> = Mutex::new(());

fn virtual_scroll_pty_lock() -> MutexGuard<'static, ()> {
    VIRTUAL_SCROLL_PTY_LOCK
        .lock()
        .expect("virtual scroll PTY lock poisoned")
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
fn pty_virtual_scrolls_with_keyboard_arrows() {
    let _guard = virtual_scroll_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_virtual_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("0008:", Duration::from_secs(2))
        .expect("initial content visible");

    for _ in 0..5 {
        host.send_str("\x1b[B").expect("ArrowDown");
    }

    host.wait_for_text("0013:", Duration::from_secs(2))
        .expect("scrolled down enough");
    assert_text_absent_for(&host, "0000:", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_virtual_scrolls_with_mouse_wheel() {
    let _guard = virtual_scroll_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_virtual_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("0008:", Duration::from_secs(2))
        .expect("initial content visible");

    // For an 80x24 PTY:
    // window at (2,3), inner at (3,4), ScrollView padding at (1,1) -> content origin at (4,5).
    host.wheel_down(4, 5).expect("wheel down");
    // Default wheel step is 3 lines.
    host.wait_for_text("0011:", Duration::from_secs(2))
        .expect("wheel scroll moved content");
    assert_text_absent_for(&host, "Virtual scroll test:", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_virtual_scrollbar_thumb_drag_scrolls_to_end() {
    let _guard = virtual_scroll_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_virtual_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Virtual scroll test:", Duration::from_secs(2))
        .expect("initial header visible");

    // Drag the thumb on the window's right border from near the top to near the bottom.
    // For an 80x24 PTY the window is at (2,3) with width 50, so the right border is x=51.
    host.drag_left(51, 5, 51, 14).expect("drag thumb down");
    host.wait_for_text("0999:", Duration::from_secs(2))
        .expect("dragged to bottom");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_virtual_scrollbar_arrow_buttons_scroll_by_one_line() {
    let _guard = virtual_scroll_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_virtual_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Virtual scroll test:", Duration::from_secs(2))
        .expect("initial header visible");

    assert_text_absent_for(&host, "0009:", Duration::from_millis(200));

    // Click the down-arrow at the bottom of the vertical scrollbar on the window's right border.
    // For an 80x24 PTY the window is at (2,3) with width 50 and height 14:
    // right border x = 2 + 50 - 1 = 51, bottom arrow y = 3 + 14 - 2 = 15.
    host.click(51, 15).expect("click vbar down arrow");
    host.wait_for_text("0009:", Duration::from_secs(2))
        .expect("down arrow scrolls by one line");
    assert_text_absent_for(&host, "Virtual scroll test:", Duration::from_millis(200));

    // Click the up-arrow at the top of the vertical scrollbar to return to the top.
    // Up arrow is the first bar cell: y = window.y + 1 = 4.
    host.click(51, 4).expect("click vbar up arrow");
    host.wait_for_text("Virtual scroll test:", Duration::from_secs(2))
        .expect("up arrow scrolls back by one line");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_virtual_horizontal_scrollbar_arrow_button_scrolls_by_one_column() {
    let _guard = virtual_scroll_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_virtual_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("[col-00]", Duration::from_secs(2))
        .expect("initial content visible");

    // `[col-05]` is not fully visible at the initial scroll position.
    assert_text_absent_for(&host, "[col-05]", Duration::from_millis(200));

    // Click the right arrow on the horizontal scrollbar (bottom border, one cell left of the
    // bottom-right corner).
    // window at (2,3) with width 50 and height 14 -> right arrow at (50, 16).
    for _ in 0..20 {
        host.click(50, 16).expect("click hbar right arrow");
    }

    host.wait_for_text("[col-05]", Duration::from_secs(2))
        .expect("right arrow scrolls by small increments");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_virtual_horizontal_scrollbar_thumb_drag_scrolls_to_end() {
    let _guard = virtual_scroll_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_virtual_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("[col-00]", Duration::from_secs(2))
        .expect("initial content visible");

    // Drag the horizontal scrollbar thumb on the bottom border from near the left to near the right.
    // Start one cell to the right of the left arrow, end one cell to the left of the right arrow.
    host.drag_left(4, 16, 50, 16).expect("drag thumb right");

    host.wait_for_text("[col-39]", Duration::from_secs(2))
        .expect("dragged to the far right");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
