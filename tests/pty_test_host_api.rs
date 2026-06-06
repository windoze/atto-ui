use std::time::Duration;

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost, ScreenRegion};

fn wait_for_line(host: &PtyTestHost, needle: &str) {
    host.wait_for_screen(
        |screen| screen.iter().any(|line| line.contains(needle)),
        Duration::from_secs(2),
    )
    .unwrap_or_else(|err| panic!("waiting for {needle:?}: {err}"));
}

#[test]
fn pty_test_host_input_resize_and_snapshot_apis() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--input-api"], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Input API fixture", Duration::from_secs(2))
        .expect("initial render");

    let snapshot = host.screen_snapshot().expect("screen snapshot");
    assert!(
        snapshot.iter().any(|line| line == "Input API fixture"),
        "snapshot should contain fixture heading: {snapshot:?}"
    );

    let region = host
        .region_snapshot(ScreenRegion::new(0, 0, 24, 2))
        .expect("region snapshot");
    assert_eq!(region, vec!["Input API fixture", "size: 80x24"]);

    let (cursor_row, cursor_col) = host.cursor_position().expect("cursor position");
    assert!(cursor_row < host.rows());
    assert!(cursor_col < host.cols());

    host.click_with_mods(2, 5, KeyModifiers::SHIFT | KeyModifiers::CONTROL)
        .expect("modified click");
    wait_for_line(&host, "mouse:up-left@2,5 mods=SHIFT|CONTROL");

    host.right_click(3, 6).expect("right click");
    wait_for_line(&host, "mouse:up-right@3,6 mods=NONE");

    host.middle_click(4, 7).expect("middle click");
    wait_for_line(&host, "mouse:up-middle@4,7 mods=NONE");

    host.mouse_move(5, 8).expect("mouse move");
    wait_for_line(&host, "mouse:moved@5,8 mods=NONE");

    host.scroll_left(6, 9).expect("horizontal scroll left");
    wait_for_line(&host, "mouse:scroll-left@6,9 mods=NONE");

    host.scroll_right(7, 10).expect("horizontal scroll right");
    wait_for_line(&host, "mouse:scroll-right@7,10 mods=NONE");

    host.key_with_mods(KeyCode::F(3), KeyModifiers::SHIFT | KeyModifiers::ALT)
        .expect("modified key");
    wait_for_line(&host, "key:F(3) kind:press mods=SHIFT|ALT");

    host.resize(100, 30).expect("resize PTY");
    assert_eq!(host.cols(), 100);
    assert_eq!(host.rows(), 30);
    wait_for_line(&host, "size: 100x30");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
