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
fn pty_modal_blocks_desktop_shortcuts() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    // Open the menu bar and trigger Help -> About, which opens a modal window.
    host.send_str("\x1b[21~").expect("F10");
    host.wait_for_text("Quit", Duration::from_secs(2))
        .expect("menu dropdown visible");
    host.send_str("\x1b[C").expect("right arrow (Help)");
    host.send_str("\x1b[B").expect("down arrow (About)");
    host.send(b"\r").expect("enter (open About)");
    host.wait_for_text("About modal", Duration::from_secs(2))
        .expect("modal opened");

    // Ctrl+W should be ignored while a modal is open (no window management mode).
    host.send_ctrl('w').expect("Ctrl+W");
    assert_text_absent_for(&host, "Window:", Duration::from_millis(200));

    // F10 should be ignored while a modal is open (no menu activation).
    host.send_str("\x1b[21~").expect("F10");
    assert_text_absent_for(&host, "Menu:", Duration::from_millis(200));

    // Close modal, then ensure desktop shortcuts work again.
    //
    // Avoid sending `Esc` here: many terminal key sequences (including `F10`) begin with ESC,
    // which can race with Crossterm's escape-sequence disambiguation.
    host.send(b"\r").expect("enter (close modal)");
    host.send_str("\x1b[21~").expect("F10");
    host.wait_for_text("Menu:", Duration::from_secs(2))
        .expect("menu works after modal closed");

    host.send(b"\x1b").expect("esc (close menu)");
    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
