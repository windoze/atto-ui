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

#[test]
fn pty_escape_cancels_background_task_and_ui_remains_interactive() {
    let bin = env!("CARGO_BIN_EXE_snapshot_async_app");
    let mut host = PtyTestHost::spawn(bin, &["--cancellable"], 80, 24).expect("spawn PTY app");

    host.wait_for_text("Task cancellation fixture", Duration::from_secs(2))
        .expect("initial render");
    host.wait_for_text("Running: false", Duration::from_secs(2))
        .expect("initial running state");

    host.send_str("s").expect("start task");
    host.wait_for_text("Task: running", Duration::from_secs(2))
        .expect("task started");
    host.wait_for_text("Running: true", Duration::from_secs(2))
        .expect("running state true");

    host.send(b"\x1b").expect("cancel task");
    host.wait_for_text("Task: cancelled", Duration::from_secs(2))
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
