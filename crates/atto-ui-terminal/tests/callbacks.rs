use std::sync::mpsc;
use std::time::Duration;

use atto_ui_terminal::{TerminalClipboardCopy, TerminalEmulator};

#[test]
fn terminal_callbacks_report_window_title_and_icon_name() {
    let (title_tx, title_rx) = mpsc::channel();
    let (icon_tx, icon_rx) = mpsc::channel();
    let terminal = TerminalEmulator::new()
        .on_window_title(move |title| {
            title_tx.send(title.to_string()).expect("send title");
        })
        .on_window_icon_name(move |icon_name| {
            icon_tx.send(icon_name.to_string()).expect("send icon name");
        });
    let handle = terminal.handle();

    assert_eq!(handle.window_title(), None);
    assert_eq!(handle.window_icon_name(), None);

    handle.process_output_str("\x1b]2;Project Shell\x07");
    assert_eq!(
        title_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("title callback"),
        "Project Shell"
    );
    assert_eq!(handle.window_title().as_deref(), Some("Project Shell"));
    assert_eq!(handle.window_icon_name(), None);

    handle.process_output_str("\x1b]1;Shell Icon\x07");
    assert_eq!(
        icon_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("icon callback"),
        "Shell Icon"
    );
    assert_eq!(handle.window_icon_name().as_deref(), Some("Shell Icon"));
}

#[test]
fn terminal_osc_zero_updates_title_and_icon_name() {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    handle.process_output_str("\x1b]0;Unified Title\x07");

    assert_eq!(handle.window_title().as_deref(), Some("Unified Title"));
    assert_eq!(handle.window_icon_name().as_deref(), Some("Unified Title"));
}

#[test]
fn terminal_callbacks_report_audible_bells() {
    let (tx, rx) = mpsc::channel();
    let terminal = TerminalEmulator::new().on_audible_bell(move || {
        tx.send(()).expect("send bell event");
    });
    let handle = terminal.handle();

    assert_eq!(handle.audible_bell_count(), 0);
    handle.process_output(b"\x07\x07");

    rx.recv_timeout(Duration::from_secs(1))
        .expect("first bell callback");
    rx.recv_timeout(Duration::from_secs(1))
        .expect("second bell callback");
    assert_eq!(handle.audible_bell_count(), 2);
}

#[test]
fn terminal_callbacks_report_clipboard_copy_requests() {
    let (tx, rx) = mpsc::channel();
    let terminal = TerminalEmulator::new().on_clipboard_copy(move |copy| {
        tx.send(copy.clone()).expect("send clipboard copy");
    });
    let handle = terminal.handle();
    let expected = TerminalClipboardCopy {
        selector: b"c".to_vec(),
        data: b"aGVsbG8=".to_vec(),
    };

    assert_eq!(handle.last_clipboard_copy(), None);
    handle.process_output(b"\x1b]52;c;aGVsbG8=\x07");

    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1))
            .expect("clipboard callback"),
        expected
    );
    assert_eq!(handle.last_clipboard_copy(), Some(expected));
}

#[test]
fn terminal_callbacks_are_observable_from_spawned_shell_output() {
    let (title_tx, title_rx) = mpsc::channel();
    let (bell_tx, bell_rx) = mpsc::channel();
    let (clipboard_tx, clipboard_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel();
    let mut terminal = TerminalEmulator::new()
        .on_window_title(move |title| {
            title_tx.send(title.to_string()).expect("send title");
        })
        .on_audible_bell(move || {
            bell_tx.send(()).expect("send bell");
        })
        .on_clipboard_copy(move |copy| {
            clipboard_tx
                .send(copy.clone())
                .expect("send clipboard copy");
        })
        .on_exit(move |status| {
            exit_tx.send(status.exit_code()).expect("send exit code");
        });
    let handle = terminal.handle();
    let expected_clipboard = TerminalClipboardCopy {
        selector: b"c".to_vec(),
        data: b"c3Bhd25lZA==".to_vec(),
    };
    let args = vec![
        "-c".to_string(),
        "printf '\\033]2;Spawned Shell\\007\\007\\033]52;c;c3Bhd25lZA==\\007'; sleep 1; exit 12"
            .to_string(),
    ];

    terminal
        .spawn_process("/bin/sh", &args)
        .expect("spawn shell command");

    assert!(handle.is_running());
    assert_eq!(
        title_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("title callback"),
        "Spawned Shell"
    );
    bell_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("bell callback");
    assert_eq!(
        clipboard_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("clipboard callback"),
        expected_clipboard
    );
    assert_eq!(handle.window_title().as_deref(), Some("Spawned Shell"));
    assert_eq!(handle.audible_bell_count(), 1);
    assert_eq!(handle.last_clipboard_copy(), Some(expected_clipboard));
    assert_eq!(
        exit_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("exit callback"),
        12
    );
    assert!(!handle.is_running());
    assert_eq!(
        handle.exit_status().map(|status| status.exit_code()),
        Some(12)
    );
}
