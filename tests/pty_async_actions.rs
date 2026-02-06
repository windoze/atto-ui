use std::time::Duration;

use atto_ui_test_host::PtyTestHost;

#[test]
fn pty_async_actions_dispatch_to_main_thread() {
    let bin = env!("CARGO_BIN_EXE_snapshot_async_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("Async: idle", Duration::from_secs(2))
        .expect("initial render");

    host.send_str("s").expect("trigger background action");
    host.wait_for_text("Async: done", Duration::from_secs(2))
        .expect("action processed on main thread");

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
