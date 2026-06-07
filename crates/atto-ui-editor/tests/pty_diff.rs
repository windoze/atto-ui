use std::time::Duration;

use atto_ui_test_host::PtyTestHost;

const DIVIDER: char = '│';

fn spawn(cols: u16, rows: u16) -> PtyTestHost {
    let bin = env!("CARGO_BIN_EXE_snapshot_diff_app");
    let host = PtyTestHost::spawn(bin, &[], cols, rows).expect("spawn PTY diff app");
    // Wait for the before (left) column to settle, not just the after column.
    host.wait_for_text("REMOVED_LINE", Duration::from_secs(3))
        .expect("before-side render");
    host
}

fn line_with<'a>(screen: &'a str, needle: &str) -> Option<&'a str> {
    screen.lines().find(|l| l.contains(needle))
}

#[test]
fn pty_diff_side_by_side_shows_both_sides() {
    let host = spawn(120, 30);
    let screen = host.screen_contents().expect("screen");

    assert!(
        screen.contains("REMOVED_LINE"),
        "before side removed line\n{screen}"
    );
    assert!(
        screen.contains("ADDED_LINE"),
        "after side added line\n{screen}"
    );
    assert!(
        screen.contains("OLD_TEXT") && screen.contains("NEW_TEXT"),
        "replaced line shows old and new\n{screen}"
    );
    assert!(
        screen.contains("ctx bottom five"),
        "trailing context\n{screen}"
    );

    // In side-by-side every content row crosses the vertical divider.
    let added = line_with(&screen, "ADDED_LINE").expect("added line");
    assert!(
        added.contains(DIVIDER),
        "side-by-side row has a divider\n{added:?}"
    );
}

#[test]
fn pty_diff_unified_shows_add_remove_markers() {
    let mut host = spawn(120, 30);

    host.send_str("u").expect("send u");
    // Unified mode: the row with ADDED_LINE no longer crosses a divider.
    let lines = host
        .wait_for_screen(
            |lines| {
                lines
                    .iter()
                    .find(|l| l.contains("ADDED_LINE"))
                    .is_some_and(|l| !l.contains(DIVIDER))
            },
            Duration::from_secs(3),
        )
        .expect("unified render");
    let screen = lines.join("\n");

    assert!(screen.contains("REMOVED_LINE"), "unified removed\n{screen}");
    assert!(screen.contains("NEW_TEXT"), "unified new\n{screen}");
    assert!(screen.contains("OLD_TEXT"), "unified old\n{screen}");

    let removed = line_with(&screen, "REMOVED_LINE").expect("removed line");
    assert!(removed.contains('-'), "removed marker\n{removed:?}");
    let added = line_with(&screen, "ADDED_LINE").expect("added line");
    assert!(added.contains('+'), "added marker\n{added:?}");
}

#[test]
fn pty_diff_mode_toggle_round_trip() {
    let mut host = spawn(120, 30);

    host.send_str("u").expect("send u");
    host.wait_for_screen(
        |lines| {
            lines
                .iter()
                .find(|l| l.contains("ADDED_LINE"))
                .is_some_and(|l| !l.contains(DIVIDER))
        },
        Duration::from_secs(3),
    )
    .expect("unified after toggle");

    host.send_str("s").expect("send s");
    host.wait_for_screen(
        |lines| {
            lines
                .iter()
                .find(|l| l.contains("ADDED_LINE"))
                .is_some_and(|l| l.contains(DIVIDER))
        },
        Duration::from_secs(3),
    )
    .expect("side-by-side after toggle back");
}

#[test]
fn pty_diff_side_by_side_synced_scroll() {
    // Small viewport so content overflows and scrolling is observable.
    let mut host = spawn(120, 10);

    let before = host.screen_contents().expect("screen");
    assert!(
        before.contains("ctx top one"),
        "top visible initially\n{before}"
    );

    // Wheel down over the left column; the shared offset must move both sides together.
    for _ in 0..8 {
        host.wheel_down(10, 5).expect("wheel down");
    }

    host.wait_for_screen(
        |lines| {
            let joined = lines.join("\n");
            joined.contains("ctx bottom five") && !joined.contains("ctx top one")
        },
        Duration::from_secs(3),
    )
    .expect("scrolled to bottom on both sides");
}

#[test]
fn pty_diff_splitter_drag_reflows() {
    let mut host = spawn(120, 30);

    // Drag the divider to the left; the projection must rebuild and both sides keep rendering.
    host.drag_left(60, 14, 40, 14).expect("drag divider");

    host.wait_for_screen(
        |lines| {
            let joined = lines.join("\n");
            joined.contains("REMOVED_LINE") && joined.contains("ADDED_LINE")
        },
        Duration::from_secs(3),
    )
    .expect("content still renders after divider drag");
}
