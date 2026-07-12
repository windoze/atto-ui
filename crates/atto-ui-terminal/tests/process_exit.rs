use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atto_ui_terminal::{TerminalEmulator, TerminalHandle, TerminalSessionSpec};

fn wait_for_exit(handle: &TerminalHandle, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while handle.exit_status().is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(handle.exit_status().is_some(), "subprocess did not exit");
}

fn wait_for_snapshot_text(handle: &TerminalHandle, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let snapshot = handle.snapshot();
        if snapshot.text().contains(needle) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "terminal output did not contain {needle:?}; snapshot:\n{}",
        handle.snapshot().text()
    );
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("atto-ui-terminal-{name}-{}-{nanos}", process::id()));
    fs::create_dir_all(&dir).expect("create temp cwd");
    dir
}

#[test]
fn terminal_handle_reports_running_state_and_exit_status() {
    let (tx, rx) = mpsc::channel();
    let mut terminal = TerminalEmulator::new().on_exit(move |status| {
        tx.send(status.exit_code()).expect("send exit status");
    });
    let handle = terminal.handle();

    assert!(!handle.is_running());
    assert!(handle.exit_status().is_none());

    let args = vec![
        "-c".to_string(),
        "printf terminal-running-ready; sleep 1; exit 9".to_string(),
    ];
    terminal
        .spawn_process("/bin/sh", &args)
        .expect("spawn shell command");

    assert!(handle.is_running());
    assert!(handle.exit_status().is_none());

    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5))
            .expect("exit status callback"),
        9
    );
    assert!(!handle.is_running());
    assert_eq!(
        handle.exit_status().map(|status| status.exit_code()),
        Some(9)
    );
}

#[test]
fn terminal_spawn_session_sets_environment_cwd_and_initial_size() {
    let cwd = fs::canonicalize(unique_temp_dir("spawn-env")).expect("canonical temp cwd");
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    assert!(terminal.resize(13, 120));

    let script = "printf 'TERM=%s\nCOLORTERM=%s\n' \"$TERM\" \"$COLORTERM\"; \
                  printf 'CWD='; pwd; \
                  printf 'SIZE='; stty size; \
                  exit 0";
    let spec =
        TerminalSessionSpec::command("Env", "/bin/sh", vec!["-c".to_string(), script.to_string()])
            .with_cwd(&cwd);

    terminal.spawn_session(&spec).expect("spawn shell command");
    wait_for_exit(&handle, Duration::from_secs(5));

    let text = handle.snapshot().text();
    assert!(text.contains("TERM=xterm-256color"), "snapshot:\n{text}");
    assert!(text.contains("COLORTERM=truecolor"), "snapshot:\n{text}");
    assert!(
        text.contains(&format!("CWD={}", cwd.display())),
        "snapshot:\n{text}"
    );
    assert!(text.contains("SIZE=13 120"), "snapshot:\n{text}");

    fs::remove_dir_all(cwd).expect("remove temp cwd");
}

#[test]
fn terminal_handle_resize_updates_running_pty_size() {
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    let args = vec![
        "-c".to_string(),
        "printf READY; sleep 0.2; printf 'SIZE='; stty size; exit 0".to_string(),
    ];

    terminal
        .spawn_process("/bin/sh", &args)
        .expect("spawn shell command");
    assert!(handle.resize(18, 50));

    wait_for_snapshot_text(&handle, "SIZE=18 50", Duration::from_secs(5));
    wait_for_exit(&handle, Duration::from_secs(5));
}

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
