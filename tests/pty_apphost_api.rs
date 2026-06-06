use std::time::Duration;

use atto_ui_test_host::PtyTestHost;

#[test]
fn apphost_send_event_drives_button_callbacks() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--apphost-api"], 80, 24).expect("spawn PTY app");

    host.wait_for_text("AppHost API calls: 2", Duration::from_secs(2))
        .expect("apphost fixture result");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
