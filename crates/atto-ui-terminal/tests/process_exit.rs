use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use atto_ui_terminal::TerminalEmulator;

#[test]
fn terminal_on_exit_reports_subprocess_status_once() {
    let (tx, rx) = mpsc::channel();
    let mut terminal = TerminalEmulator::new().on_exit(move |status| {
        tx.send(status.exit_code()).expect("send exit status");
    });

    let args = vec![
        "-c".to_string(),
        "printf terminal-exit-ready; exit 7".to_string(),
    ];
    terminal
        .spawn_process("/bin/sh", &args)
        .expect("spawn shell command");

    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5))
            .expect("exit status callback"),
        7
    );

    terminal.stop_process();
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "exit callback should be emitted once"
    );
}

#[test]
fn terminal_on_exit_is_distinct_from_on_close() {
    let exit_called = Arc::new(AtomicBool::new(false));
    let close_called = Arc::new(AtomicBool::new(false));

    {
        let exit_called = Arc::clone(&exit_called);
        let close_called = Arc::clone(&close_called);
        let _terminal = TerminalEmulator::new()
            .on_exit(move |_| {
                exit_called.store(true, Ordering::SeqCst);
            })
            .on_close(move || {
                close_called.store(true, Ordering::SeqCst);
            });
    }

    assert!(close_called.load(Ordering::SeqCst));
    assert!(!exit_called.load(Ordering::SeqCst));
}
