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
fn pty_foreach_root_view_renders_window_border_scrollbar() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_foreach_scroll_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("Scroll test:", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "009: line for scrolling", Duration::from_millis(200));

    // Click the down arrow on the window's right border scrollbar.
    // For an 80x24 PTY the window is at (2,3) with width 50 and height 14:
    // right border x = 2 + 50 - 1 = 51, bottom arrow y = 3 + 14 - 2 = 15.
    host.click(51, 15)?;
    host.wait_for_text("009: line for scrolling", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "Scroll test:", Duration::from_millis(200));

    // Click the up arrow to return to the top.
    // Up arrow is the first bar cell: y = window.y + 1 = 4.
    host.click(51, 4)?;
    host.wait_for_text("Scroll test:", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
