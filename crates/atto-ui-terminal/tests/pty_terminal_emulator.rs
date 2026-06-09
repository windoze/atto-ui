use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::PtyTestHost;
use unicode_width::UnicodeWidthStr;

const PTY_WAIT: Duration = Duration::from_secs(5);

static PTY_TERMINAL_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_terminal_pty() -> MutexGuard<'static, ()> {
    PTY_TERMINAL_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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

fn find_text_position(screen: &str, needle: &str) -> Option<(u16, u16)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte_idx) = line.find(needle) {
            let col = UnicodeWidthStr::width(&line[..byte_idx]);
            return Some((col as u16, row as u16));
        }
    }
    None
}

#[test]
fn pty_terminal_scrollback_and_colors() {
    let _guard = lock_terminal_pty();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("READY", PTY_WAIT)
        .expect("terminal ready");
    host.wait_for_text("RED", PTY_WAIT)
        .expect("color line visible");

    let screen = host.screen_contents().unwrap_or_default();
    let (red_x, red_y) = find_text_position(&screen, "RED").expect("find RED position");
    assert_eq!(
        host.cell_contents(red_x, red_y).expect("red cell contents"),
        "R"
    );

    host.send_ctrl('g').expect("release capture");
    host.wait_for_text("[CAPTURE OFF]", PTY_WAIT)
        .expect("capture off");

    assert_text_absent_for(&host, "SCROLL-00", Duration::from_millis(200));

    let wheel_x = 4;
    let wheel_y = 4;
    for _ in 0..10 {
        host.wheel_up(wheel_x, wheel_y)
            .expect("wheel up scrollback");
    }
    host.wait_for_text("SCROLL-00", PTY_WAIT)
        .expect("scrollback visible");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(PTY_WAIT).expect("clean exit");
}

#[test]
fn pty_terminal_capture_and_mouse_input() {
    let _guard = lock_terminal_pty();
    let bin = env!("CARGO_BIN_EXE_snapshot_terminal_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("READY", PTY_WAIT)
        .expect("terminal ready");

    host.send_str("a").expect("send a");
    host.wait_for_text("IN: a", PTY_WAIT).expect("input echoed");

    host.send_ctrl('g').expect("release capture");
    host.wait_for_text("[CAPTURE OFF]", PTY_WAIT)
        .expect("capture off");

    host.send_str("b").expect("send b");
    assert_text_absent_for(&host, "IN: b", Duration::from_millis(200));

    let click_x = 4;
    let click_y = 4;
    host.click(click_x, click_y).expect("click to recapture");
    host.wait_for_text("[CAPTURE ON]", PTY_WAIT)
        .expect("capture on");

    host.send_str("c").expect("send c");
    host.wait_for_text("IN: c", PTY_WAIT).expect("input echoed");

    host.click(click_x, click_y).expect("mouse input");
    host.wait_for_text("IN: <1B>[<0;", PTY_WAIT)
        .expect("mouse input encoded");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(PTY_WAIT).expect("clean exit");
}
