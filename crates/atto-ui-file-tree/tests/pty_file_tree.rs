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
fn pty_file_tree_expands_and_collapses() {
    let bin = env!("CARGO_BIN_EXE_snapshot_file_tree_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("main.rs", Duration::from_secs(2))
        .expect("initial tree visible");

    host.click(6, 8).expect("select assets");
    host.wait_for_text("logo.png", Duration::from_secs(2))
        .expect("expanded assets");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_file_tree_rename_and_delete() {
    let bin = env!("CARGO_BIN_EXE_snapshot_file_tree_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("README.md", Duration::from_secs(2))
        .expect("tree visible");

    host.click(6, 9).expect("select README");
    host.send_str("r").expect("rename");
    host.send_str("README_NEW.md").expect("new name");
    host.send_str("\r").expect("confirm rename");

    host.wait_for_text("README_NEW.md", Duration::from_secs(2))
        .expect("renamed entry visible");
    assert_text_absent_for(&host, "README.md", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
