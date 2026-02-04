use std::time::Duration;

use chatty_test_host::PtyTestHost;

#[test]
fn pty_window_border_hides_overlapped_wide_glyph() {
    let bin = env!("CARGO_BIN_EXE_snapshot_wide_overlap_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    // Wait for a stable, non-overlapped wide glyph in the background window.
    host.wait_for_text("好", Duration::from_secs(2))
        .expect("initial render");

    // Sanity check: a wide glyph that's not overlapped should still render.
    let visible = host.cell_contents(4, 4).expect("cell (4,4)");
    assert_eq!(
        visible,
        "好",
        "expected fully-visible wide glyph at (4,4)\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );

    // The background wide glyph `你` starts at (8,4) and spans (8,4)-(9,4).
    // The foreground window's left border is at x=9, so it overlaps the right half.
    // Expected behavior: hide the wide glyph completely and show the border.
    let border = host.cell_contents(9, 4).expect("cell (9,4)");
    assert_eq!(
        border,
        "│",
        "expected window border at (9,4)\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );

    let hidden = host.cell_contents(8, 4).expect("cell (8,4)");
    assert_eq!(
        hidden,
        " ",
        "expected overlapped wide glyph to be hidden at (8,4)\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
