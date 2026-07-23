use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_terminal::{
    TerminalClipboardCopy, TerminalCommandBlock, TerminalEmulator, TerminalShellIntegration,
};
use portable_pty::CommandBuilder;

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
fn terminal_empty_osc_title_clears_window_title() {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    handle.process_output_str("\x1b]2;Claude Code\x07");
    assert_eq!(handle.window_title().as_deref(), Some("Claude Code"));

    // 应用退出时用空的 OSC 0/2 清空标题,应归一化为 None,而不是保留成 Some("")。
    handle.process_output_str("\x1b]2;\x07");
    assert_eq!(handle.window_title(), None);
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
    let copied = Arc::new(Mutex::new(Vec::new()));
    let copied_for_clipboard = Arc::clone(&copied);
    let terminal = TerminalEmulator::new()
        .system_clipboard(move |text: &str| {
            copied_for_clipboard
                .lock()
                .expect("clipboard lock")
                .push(text.to_string());
            Ok(())
        })
        .on_clipboard_copy(move |copy| {
            tx.send(copy.clone()).expect("send clipboard copy");
        });
    let handle = terminal.handle();
    let expected = TerminalClipboardCopy {
        selector: b"c".to_vec(),
        data: b"aGVsbG8=".to_vec(),
    };

    assert_eq!(handle.last_clipboard_copy(), None);
    handle.process_output(b"\x1b]52;c;aGVsbG8=\x07");

    assert_eq!(expected.decoded_text().expect("decode OSC 52"), "hello");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1))
            .expect("clipboard callback"),
        expected
    );
    assert_eq!(handle.last_clipboard_copy(), Some(expected));
    assert_eq!(handle.copied_text().as_deref(), Some("hello"));
    assert_eq!(
        handle.last_system_clipboard_text().as_deref(),
        Some("hello")
    );
    assert_eq!(handle.last_system_clipboard_error(), None);
    assert_eq!(copied.lock().expect("clipboard lock").as_slice(), ["hello"]);
}

#[test]
fn terminal_tmux_dcs_passthrough_unwraps_osc52_clipboard_copy() {
    let (tx, rx) = mpsc::channel();
    let copied = Arc::new(Mutex::new(Vec::new()));
    let copied_for_clipboard = Arc::clone(&copied);
    let terminal = TerminalEmulator::new()
        .system_clipboard(move |text: &str| {
            copied_for_clipboard
                .lock()
                .expect("clipboard lock")
                .push(text.to_string());
            Ok(())
        })
        .on_clipboard_copy(move |copy| {
            tx.send(copy.clone()).expect("send clipboard copy");
        });
    let handle = terminal.handle();
    let expected = TerminalClipboardCopy {
        selector: b"c".to_vec(),
        data: b"aGVsbG8=".to_vec(),
    };

    handle.process_output(b"\x1bPtmux;\x1b\x1b]52;c;aGVsbG8=\x07\x1b\\");

    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1))
            .expect("clipboard callback"),
        expected
    );
    assert_eq!(handle.last_clipboard_copy(), Some(expected));
    assert_eq!(handle.copied_text().as_deref(), Some("hello"));
    assert_eq!(
        handle.last_system_clipboard_text().as_deref(),
        Some("hello")
    );
    assert_eq!(handle.last_system_clipboard_error(), None);
    assert_eq!(copied.lock().expect("clipboard lock").as_slice(), ["hello"]);
}

#[test]
fn terminal_tmux_dcs_passthrough_handles_split_packets() {
    let copied = Arc::new(Mutex::new(Vec::new()));
    let copied_for_clipboard = Arc::clone(&copied);
    let terminal = TerminalEmulator::new().system_clipboard(move |text: &str| {
        copied_for_clipboard
            .lock()
            .expect("clipboard lock")
            .push(text.to_string());
        Ok(())
    });
    let handle = terminal.handle();

    handle.process_output(b"\x1bPtmux;\x1b");
    assert_eq!(handle.last_clipboard_copy(), None);
    assert_eq!(handle.last_system_clipboard_text(), None);

    handle.process_output(b"\x1b]52;c;Y2h1bms=\x07\x1b\\");

    assert_eq!(handle.copied_text().as_deref(), Some("chunk"));
    assert_eq!(
        handle.last_system_clipboard_text().as_deref(),
        Some("chunk")
    );
    assert_eq!(copied.lock().expect("clipboard lock").as_slice(), ["chunk"]);
}

#[test]
fn terminal_malformed_tmux_dcs_passthrough_does_not_sync_clipboard() {
    let copied = Arc::new(Mutex::new(Vec::new()));
    let copied_for_clipboard = Arc::clone(&copied);
    let terminal = TerminalEmulator::new().system_clipboard(move |text: &str| {
        copied_for_clipboard
            .lock()
            .expect("clipboard lock")
            .push(text.to_string());
        Ok(())
    });
    let handle = terminal.handle();

    handle.process_output(b"\x1bPtmux;\x1b]52;c;bm90LWNvcHk=\x07\x1b\\");

    assert_eq!(handle.last_clipboard_copy(), None);
    assert_eq!(handle.copied_text(), None);
    assert_eq!(handle.last_system_clipboard_text(), None);
    assert!(copied.lock().expect("clipboard lock").is_empty());
}

#[test]
fn terminal_non_tmux_dcs_passthrough_does_not_sync_clipboard() {
    let copied = Arc::new(Mutex::new(Vec::new()));
    let copied_for_clipboard = Arc::clone(&copied);
    let terminal = TerminalEmulator::new().system_clipboard(move |text: &str| {
        copied_for_clipboard
            .lock()
            .expect("clipboard lock")
            .push(text.to_string());
        Ok(())
    });
    let handle = terminal.handle();

    handle.process_output(b"\x1bPnot-tmux;\x1b\x1b]52;c;bm90LWNvcHk=\x07\x1b\\");

    assert_eq!(handle.last_clipboard_copy(), None);
    assert_eq!(handle.copied_text(), None);
    assert_eq!(handle.last_system_clipboard_text(), None);
    assert!(copied.lock().expect("clipboard lock").is_empty());
}

#[test]
fn terminal_osc52_non_clipboard_selector_does_not_sync_system_clipboard() {
    let (tx, rx) = mpsc::channel();
    let copied = Arc::new(Mutex::new(Vec::new()));
    let copied_for_clipboard = Arc::clone(&copied);
    let terminal = TerminalEmulator::new()
        .system_clipboard(move |text: &str| {
            copied_for_clipboard
                .lock()
                .expect("clipboard lock")
                .push(text.to_string());
            Ok(())
        })
        .on_clipboard_copy(move |copy| {
            tx.send(copy.clone()).expect("send clipboard copy");
        });
    let handle = terminal.handle();
    let expected = TerminalClipboardCopy {
        selector: b"p".to_vec(),
        data: b"cHJpbWFyeQ==".to_vec(),
    };

    handle.process_output(b"\x1b]52;p;cHJpbWFyeQ==\x07");

    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1))
            .expect("clipboard callback"),
        expected
    );
    assert_eq!(handle.last_clipboard_copy(), Some(expected));
    assert_eq!(handle.copied_text(), None);
    assert_eq!(handle.last_system_clipboard_text(), None);
    assert!(copied.lock().expect("clipboard lock").is_empty());
}

#[test]
fn terminal_command_blocks_are_queryable_and_report_finished_callback() {
    let (tx, rx) = mpsc::channel();
    let terminal = TerminalEmulator::new().on_command_finished(move |block| {
        tx.send(block.clone()).expect("send command block");
    });
    let handle = terminal.handle();
    let expected = TerminalCommandBlock {
        prompt_start: Some(0),
        prompt_start_col: Some(0),
        command_start: Some(0),
        command_start_col: Some(7),
        output_start: Some(1),
        output_start_col: Some(0),
        end: Some(2),
        end_col: Some(0),
        exit_code: Some(42),
        cwd: Some("/tmp/project one".to_string()),
    };

    assert!(handle.command_blocks().is_empty());
    assert_eq!(handle.last_exit_code(), None);

    handle.process_output_str(
        "\x1b]7;file://host/tmp/project%20one\x07\
         \x1b]133;A\x07$ false\
         \x1b]133;B\x07\r\n\
         \x1b]133;C\x07boom\r\n\
         \x1b]133;D;42\x07",
    );

    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1))
            .expect("command finished callback"),
        expected
    );
    assert_eq!(handle.command_blocks(), vec![expected]);
    assert_eq!(handle.last_exit_code(), Some(42));
}

#[test]
fn terminal_command_exit_code_is_distinct_from_process_exit_status() {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    handle.process_output_str(
        "\x1b]133;A\x07$ false\x1b]133;B\x07\r\n\
         \x1b]133;C\x07boom\r\n\
         \x1b]133;D;42\x07",
    );

    assert_eq!(handle.last_exit_code(), Some(42));
    assert!(handle.exit_status().is_none());
    assert!(!handle.is_running());
}

#[test]
fn terminal_osc133_state_machine_tracks_multiple_blocks_and_cwd_updates() {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    handle.process_output_str(
        "\x1b]7;file://host/home/one\x07\
         \x1b]133;A\x07one$ \x1b]133;B\x07echo one\r\n\
         \x1b]133;C\x07one\r\n\
         \x1b]133;D;0\x07\r\n\
         \x1b]133;A\x07two$ \x1b]7;file:///tmp/two%20words\x07\x1b]133;B\x07echo two\r\n\
         \x1b]133;C\x07two\r\n\
         \x1b]133;D;5\x07",
    );

    let blocks = handle.command_blocks();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].cwd.as_deref(), Some("/home/one"));
    assert_eq!(blocks[0].exit_code, Some(0));
    assert_eq!(blocks[1].cwd.as_deref(), Some("/tmp/two words"));
    assert_eq!(blocks[1].exit_code, Some(5));
    assert!(
        blocks[1].prompt_start.expect("second prompt row")
            > blocks[0].prompt_start.expect("first prompt row")
    );
    assert_eq!(handle.last_exit_code(), Some(5));
}

#[test]
fn terminal_osc133_unknown_markers_do_not_create_command_blocks() {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    handle.process_output_str(
        "\x1b]133;Z\x07plain output\r\n\
         \x1b]7;http://example.invalid/tmp\x07still plain\r\n",
    );

    assert!(handle.command_blocks().is_empty());
    assert_eq!(handle.last_exit_code(), None);
}

#[test]
fn terminal_command_block_queries_degrade_without_osc_markers() {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    handle.process_output_str("plain output\r\nwithout shell integration\r\n");

    assert!(handle.command_blocks().is_empty());
    assert_eq!(handle.last_exit_code(), None);
}

#[test]
fn terminal_callbacks_are_observable_from_spawned_shell_output() {
    let (title_tx, title_rx) = mpsc::channel();
    let (bell_tx, bell_rx) = mpsc::channel();
    let (clipboard_tx, clipboard_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel();
    let system_clipboard = Arc::new(Mutex::new(Vec::new()));
    let system_clipboard_for_callback = Arc::clone(&system_clipboard);
    let mut terminal = TerminalEmulator::new()
        .system_clipboard(move |text: &str| {
            system_clipboard_for_callback
                .lock()
                .expect("clipboard lock")
                .push(text.to_string());
            Ok(())
        })
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
    assert_eq!(handle.copied_text().as_deref(), Some("spawned"));
    assert_eq!(
        system_clipboard.lock().expect("clipboard lock").as_slice(),
        ["spawned"]
    );
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

#[test]
fn terminal_shell_integration_can_be_injected_into_spawned_bash() {
    if !Path::new("/bin/bash").exists() {
        return;
    }

    let mut terminal =
        TerminalEmulator::new().shell_integration(TerminalShellIntegration::enabled());
    let handle = terminal.handle();
    let mut cmd = CommandBuilder::new("/bin/bash");
    cmd.env("ATTO_UI_SHELL_INTEGRATION_NO_USER_RC", "1");
    terminal.spawn_command(cmd).expect("spawn integrated bash");

    handle.send_input_bytes(b"printf 'shell-integration-ok\\n'\nexit 23\n");
    let deadline = Instant::now() + Duration::from_secs(5);
    while handle.exit_status().is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(handle.last_shell_integration_error(), None);
    assert_eq!(
        handle.exit_status().map(|status| status.exit_code()),
        Some(23)
    );
    assert!(handle.snapshot().text().contains("shell-integration-ok"));
    assert!(
        handle
            .command_blocks()
            .iter()
            .any(|block| block.exit_code == Some(0)),
        "injected integration should report at least the completed printf command"
    );
}

#[test]
fn injected_bash_command_block_recovers_command_text_without_prompt() {
    // Regression: with real shell integration, "Copy command" / "Rerun" must
    // recover the *typed command* (not empty, not the prompt), i.e. the OSC 133
    // B marker must land at the end of the prompt. Previously B and C were
    // emitted together after Enter, collapsing the command range to empty.
    if !Path::new("/bin/bash").exists() {
        return;
    }

    let mut terminal =
        TerminalEmulator::new().shell_integration(TerminalShellIntegration::enabled());
    let handle = terminal.handle();
    let mut cmd = CommandBuilder::new("/bin/bash");
    cmd.env("ATTO_UI_SHELL_INTEGRATION_NO_USER_RC", "1");
    // A minimal, stable prompt so the recovered command excludes prompt glyphs.
    cmd.env("PS1", "PROMPT> ");
    terminal.spawn_command(cmd).expect("spawn integrated bash");

    handle.send_input_bytes(b"echo copy-me\n");

    // Wait for a completed command block whose command text is recoverable.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut command_text = None;
    while Instant::now() < deadline {
        if handle
            .command_blocks()
            .iter()
            .any(|block| block.exit_code.is_some())
            && let Some(text) = handle.copy_command_block_command(0)
            && text.contains("echo copy-me")
        {
            command_text = Some(text);
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    handle.send_input_bytes(b"exit\n");

    let command_text = command_text.expect("command text should be recoverable and non-empty");
    assert!(
        command_text.contains("echo copy-me"),
        "recovered command {command_text:?} should contain the typed command"
    );
    assert!(
        !command_text.contains("PROMPT>"),
        "recovered command {command_text:?} should not include the prompt"
    );
}
