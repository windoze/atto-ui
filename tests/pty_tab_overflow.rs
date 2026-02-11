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

fn find_window_bounds(screen: &str, needle: &str) -> Option<(u16, u16, u16)> {
    for (row, line) in screen.lines().enumerate() {
        if !line.contains(needle) {
            continue;
        }
        let mut left: Option<usize> = None;
        let mut right: Option<usize> = None;
        for (idx, ch) in line.chars().enumerate() {
            if left.is_none() && (ch == '┌' || ch == '╔' || ch == '+') {
                left = Some(idx);
            }
            if ch == '┐' || ch == '╗' || ch == '+' {
                right = Some(idx);
            }
        }
        let left = left?;
        let right = right?;
        return Some((row as u16, left as u16, right as u16));
    }
    None
}

fn wait_for_window_bounds(host: &PtyTestHost, needle: &str, timeout: Duration) -> (u16, u16, u16) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if let Some(bounds) = find_window_bounds(&screen, needle) {
            return bounds;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for window bounds containing {needle:?}");
}

fn find_marker_pos(screen: &str, line_contains: &str, marker: char) -> Option<(u16, u16)> {
    for (row, line) in screen.lines().enumerate() {
        if !line.contains(line_contains) {
            continue;
        }
        for (col, ch) in line.chars().enumerate() {
            if ch == marker {
                return Some((row as u16, col as u16));
            }
        }
    }
    None
}

fn wait_for_marker_pos(
    host: &PtyTestHost,
    line_contains: &str,
    marker: char,
    timeout: Duration,
) -> (u16, u16) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if let Some(pos) = find_marker_pos(&screen, line_contains, marker) {
            return pos;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for marker {marker:?} on line {line_contains:?}");
}

#[test]
fn pty_tab_view_overflow_markers_scroll_titles() {
    let bin = env!("CARGO_BIN_EXE_snapshot_tab_overflow_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    let (title_y, left_border, right_border) =
        wait_for_window_bounds(&host, "TabViewOverflow", Duration::from_secs(2));
    let header_y = title_y.saturating_add(1);
    let header_left = left_border.saturating_add(1);
    let header_right = right_border.saturating_sub(1);

    host.wait_for_text("Tab01", Duration::from_secs(2))
        .expect("initial tab titles visible");
    assert_text_absent_for(&host, "Tab07", Duration::from_millis(200));

    for _ in 0..8 {
        host.click(header_right, header_y)
            .expect("click tab view right marker");
    }

    host.wait_for_text("Tab07", Duration::from_secs(2))
        .expect("right marker scrolls to later tabs");
    assert_text_absent_for(&host, "Tab01", Duration::from_millis(200));

    for _ in 0..8 {
        host.click(header_left, header_y)
            .expect("click tab view left marker");
    }

    host.wait_for_text("Tab01", Duration::from_secs(2))
        .expect("left marker scrolls back to first tab");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_tab_window_overflow_markers_scroll_titles() {
    let bin = env!("CARGO_BIN_EXE_snapshot_tab_overflow_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    let (title_y, title_right) = wait_for_marker_pos(&host, "Win01", '►', Duration::from_secs(2));

    host.wait_for_text("Win01", Duration::from_secs(2))
        .expect("initial window tab titles visible");
    assert_text_absent_for(&host, "Win07", Duration::from_millis(200));

    for _ in 0..8 {
        host.click(title_right, title_y)
            .expect("click tab window right marker");
    }

    host.wait_for_text("Win07", Duration::from_secs(2))
        .expect("right marker scrolls to later window tabs");
    assert_text_absent_for(&host, "Win01", Duration::from_millis(200));

    let (title_y, title_left) = wait_for_marker_pos(&host, "Win07", '◄', Duration::from_secs(2));
    for _ in 0..8 {
        host.click(title_left, title_y)
            .expect("click tab window left marker");
    }

    host.wait_for_text("Win01", Duration::from_secs(2))
        .expect("left marker scrolls back to first window tab");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
