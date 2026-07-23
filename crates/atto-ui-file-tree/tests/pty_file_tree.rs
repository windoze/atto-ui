use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::{KeyModifiers, PtyTestHost};
use unicode_width::UnicodeWidthStr;

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

fn find_text_position(host: &PtyTestHost, needle: &str) -> (u16, u16) {
    let screen = host.screen_contents().expect("screen contents");
    screen
        .lines()
        .enumerate()
        .find_map(|(y, line)| {
            line.find(needle).map(|byte_idx| {
                let x = UnicodeWidthStr::width(&line[..byte_idx]);
                (
                    x.min(u16::MAX as usize) as u16,
                    y.min(u16::MAX as usize) as u16,
                )
            })
        })
        .unwrap_or_else(|| {
            panic!("expected to find {needle:?} in screen\n--- screen ---\n{screen}")
        })
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
fn pty_file_tree_renders_git_status_badge() {
    let bin = env!("CARGO_BIN_EXE_snapshot_file_tree_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("M README.md", Duration::from_secs(2))
        .expect("modified badge visible");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_file_tree_shift_click_path_remains_interactive() {
    let bin = env!("CARGO_BIN_EXE_snapshot_file_tree_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Cargo.toml", Duration::from_secs(2))
        .expect("tree visible");

    let main_pos = find_text_position(&host, "main.rs");
    let readme_pos = find_text_position(&host, "README.md");

    host.click(main_pos.0, main_pos.1).expect("select main.rs");
    host.shift_click(readme_pos.0, readme_pos.1)
        .expect("range-select README");
    host.wait_for_text("Cargo.toml", Duration::from_secs(2))
        .expect("tree remains interactive after shift-click");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_file_tree_ctrl_click_path_remains_interactive() {
    let bin = env!("CARGO_BIN_EXE_snapshot_file_tree_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Cargo.toml", Duration::from_secs(2))
        .expect("tree visible");

    let main_pos = find_text_position(&host, "main.rs");
    let readme_pos = find_text_position(&host, "README.md");

    host.click(main_pos.0, main_pos.1).expect("select main.rs");
    host.click_with_mods(readme_pos.0, readme_pos.1, KeyModifiers::CONTROL)
        .expect("toggle README into selection");
    host.wait_for_text("Cargo.toml", Duration::from_secs(2))
        .expect("tree remains interactive after ctrl-click");

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
