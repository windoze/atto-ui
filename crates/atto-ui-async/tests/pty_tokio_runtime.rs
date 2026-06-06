#![cfg(feature = "event-stream")]

use std::time::Duration;

use atto_ui_test_host::PtyTestHost;

#[test]
fn tokio_async_task_dispatches_to_ui() {
    let bin = env!("CARGO_BIN_EXE_snapshot_tokio_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("Tokio: idle", Duration::from_secs(2))
        .expect("initial render");
    host.send_str("s").expect("trigger async action");
    host.wait_for_text("Tokio: done", Duration::from_secs(2))
        .expect("action processed on main thread");

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn tokio_escape_cancels_async_task_and_ui_remains_interactive() {
    let bin = env!("CARGO_BIN_EXE_snapshot_tokio_app");
    let mut host = PtyTestHost::spawn(bin, &["--cancellable"], 80, 24).expect("spawn PTY app");

    host.wait_for_text("Tokio async fixture", Duration::from_secs(2))
        .expect("initial render");
    host.wait_for_text("Running: false", Duration::from_secs(2))
        .expect("initial running state");

    host.send_str("c").expect("start cancellable task");
    host.wait_for_text("Tokio: running", Duration::from_secs(2))
        .expect("task started");
    host.wait_for_text("Running: true", Duration::from_secs(2))
        .expect("running state true");

    host.send(b"\x1b").expect("cancel task");
    host.wait_for_text("Tokio: cancelled", Duration::from_secs(2))
        .expect("task cancelled");
    host.wait_for_text("Running: false", Duration::from_secs(2))
        .expect("running state false");

    host.send_str("p").expect("ping after cancellation");
    host.wait_for_text("Ping: 1", Duration::from_secs(2))
        .expect("UI remains interactive");

    host.send_ctrl('q').expect("send quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
