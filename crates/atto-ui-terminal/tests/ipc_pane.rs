use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::ipc::{IpcServer, send_protocol_request};
use atto_ui::protocol::{
    PaneSelectDirection, PaneSplitDirection, ProtocolRequest, ProtocolResponse, ProtocolResult,
};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind};
use atto_ui_terminal::{
    TerminalEmulator, TerminalPaneGroup, TerminalPaneIpc, TerminalTmuxEnvironmentConfig,
    terminal_pane_ipc_handler,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

static NEXT_SOCKET_ID: AtomicUsize = AtomicUsize::new(0);

fn temp_socket_path(test_name: &str) -> PathBuf {
    let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);
    env::temp_dir().join(format!(
        "atto-ui-terminal-{test_name}-{}-{id}.sock",
        std::process::id()
    ))
}

fn screen() -> Rect {
    Rect::new(0, 0, 80, 24)
}

fn empty_desktop() -> Desktop {
    Desktop::new(Theme::dark(), MenuBar::new(vec![]))
}

fn desktop_with_group(group: TerminalPaneGroup) -> Desktop {
    let mut desktop = empty_desktop();
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Terminal Panes",
            Rect::new(2, 2, 70, 18),
            Box::new(group),
        ),
        screen(),
    );
    desktop
}

fn draw_desktop(desktop: &mut Desktop) {
    let backend = TestBackend::new(screen().width, screen().height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| desktop.draw(frame)).expect("draw");
}

fn spawn_request(
    path: PathBuf,
    request: ProtocolRequest,
) -> mpsc::Receiver<io::Result<ProtocolResponse>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let response = send_protocol_request(path, &request);
        tx.send(response).expect("send test response");
    });
    rx
}

fn drive_until_response(
    server: &mut IpcServer,
    desktop: &mut Desktop,
    rx: mpsc::Receiver<io::Result<ProtocolResponse>>,
) -> ProtocolResponse {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        server.drain_pending(desktop, screen());
        if let Ok(response) = rx.try_recv() {
            return response.expect("client response");
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for IPC response"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn wait_for_snapshot_text(
    handle: &atto_ui_terminal::TerminalHandle,
    needle: &str,
    timeout: Duration,
) {
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

fn tmux_shim_dir() -> String {
    Path::new(env!("CARGO_BIN_EXE_tmux"))
        .parent()
        .expect("tmux shim bin directory")
        .to_string_lossy()
        .into_owned()
}

fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn run_tmux_shim(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tmux"))
        .args(args)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("ATTO_UI_SOCKET")
        .output()
        .expect("run tmux shim")
}

#[test]
fn tmux_shim_reports_unsupported_subcommands() {
    let output = run_tmux_shim(&["new-session"]);

    assert!(
        !output.status.success(),
        "unsupported tmux command should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported tmux subcommand new-session"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn terminal_pane_ipc_lists_and_captures_registered_panes() {
    let socket_path = temp_socket_path("list-capture");
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str("CAPTURE READY\r\n");
    let group = TerminalPaneGroup::new(terminal);
    let panes = group.handle();
    let pane_id = panes.active_pane_id().expect("active pane").raw();
    let mut server = IpcServer::bind(&socket_path).expect("bind ipc");
    server.set_method_handler(terminal_pane_ipc_handler(TerminalPaneIpc::new(
        panes.clone(),
    )));
    let mut desktop = empty_desktop();

    let list = ProtocolRequest::list_panes("list");
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), list),
    );
    match response.result {
        Some(ProtocolResult::ListPanes(panes)) => {
            assert_eq!(panes.len(), 1);
            assert_eq!(panes[0].pane_id, pane_id);
            assert!(panes[0].is_active);
        }
        other => panic!("expected list_panes result, got {other:?}"),
    }

    let capture = ProtocolRequest::capture_pane("capture", pane_id);
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path, capture),
    );
    match response.result {
        Some(ProtocolResult::CapturePane(capture)) => {
            assert_eq!(capture.pane_id, pane_id);
            assert!(capture.text().contains("CAPTURE READY"));
        }
        other => panic!("expected capture_pane result, got {other:?}"),
    }
}

#[test]
fn terminal_pane_ipc_send_keys_reaches_target_subprocess_and_capture_reads_echo() {
    let socket_path = temp_socket_path("send-capture");
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    let script = concat!(
        "printf 'READY\\r\\n'; ",
        "while IFS= read -r line; do ",
        "printf 'ECHO:%s\\r\\n' \"$line\"; ",
        "[ \"$line\" = quit ] && exit 0; ",
        "done"
    );
    let args = vec!["-c".to_string(), script.to_string()];
    terminal
        .spawn_process("/bin/sh", &args)
        .expect("spawn echo shell");
    wait_for_snapshot_text(&handle, "READY", Duration::from_secs(5));

    let group = TerminalPaneGroup::new(terminal);
    let panes = group.handle();
    let pane_id = panes.active_pane_id().expect("active pane").raw();
    let mut server = IpcServer::bind(&socket_path).expect("bind ipc");
    server.set_method_handler(terminal_pane_ipc_handler(TerminalPaneIpc::new(
        panes.clone(),
    )));
    let mut desktop = empty_desktop();

    let send = ProtocolRequest::send_keys("send", pane_id, b"hello\n".to_vec());
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), send),
    );
    match response.result {
        Some(ProtocolResult::SendKeys(result)) => {
            assert_eq!(result.pane_id, pane_id);
            assert_eq!(result.byte_count, b"hello\n".len());
        }
        other => panic!("expected send_keys result, got {other:?}"),
    }

    wait_for_snapshot_text(&handle, "ECHO:hello", Duration::from_secs(5));
    let capture = ProtocolRequest::capture_pane("capture", pane_id);
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path, capture),
    );
    match response.result {
        Some(ProtocolResult::CapturePane(capture)) => {
            assert!(capture.text().contains("ECHO:hello"));
        }
        other => panic!("expected capture_pane result, got {other:?}"),
    }

    handle.send_input_bytes(b"quit\n");
}

#[test]
fn tmux_shim_from_child_path_drives_native_pane_methods() {
    let socket_path = temp_socket_path("tmux-shim-child");
    let start_path = socket_path.with_extension("start");
    let script = format!(
        concat!(
            "while [ ! -f {start} ]; do sleep 0.01; done; ",
            "printf 'READY\\r\\n'; ",
            "tmux capture-pane -p; ",
            "tmux send-keys -t \"$TMUX_PANE\" \"printf FROM_SEND\" Enter; ",
            "tmux split-window -h; ",
            "printf 'SPLIT_DONE\\r\\n'; ",
            "exec /bin/sh"
        ),
        start = sh_quote(&start_path)
    );
    // Pane ids are allocated when the group is created, so build the group
    // first and inject the *actual* pane id as `$TMUX_PANE`; otherwise the
    // shim's `capture-pane`/`send-keys` would target a non-existent pane.
    let mut group = TerminalPaneGroup::new(TerminalEmulator::new());
    let panes = group.handle();
    let pane_id = panes.active_pane_id().expect("active pane").raw();
    let handle = panes
        .active_terminal_handle()
        .expect("active terminal handle");
    handle.set_tmux_environment(TerminalTmuxEnvironmentConfig {
        inject: true,
        socket_path: socket_path.to_string_lossy().into_owned(),
        shim_path: Some(tmux_shim_dir()),
        server_pid: Some(std::process::id()),
        session_id: 1,
        pane_id,
        override_term: false,
    });
    group
        .with_active_terminal_mut(|terminal| {
            terminal.spawn_process("/bin/sh", &["-c".to_string(), script])
        })
        .expect("active pane present")
        .expect("spawn tmux shim probe shell");
    let mut desktop = desktop_with_group(group);
    draw_desktop(&mut desktop);

    let mut server = IpcServer::bind(&socket_path).expect("bind ipc");
    server.set_method_handler(terminal_pane_ipc_handler(TerminalPaneIpc::new(
        panes.clone(),
    )));
    fs::write(&start_path, b"go").expect("release tmux shim probe shell");
    wait_for_snapshot_text(&handle, "READY", Duration::from_secs(5));

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut text = String::new();
    while Instant::now() < deadline {
        server.drain_pending(&mut desktop, screen());
        draw_desktop(&mut desktop);
        text = handle.snapshot().text();
        if panes.pane_count() == 2
            && text.contains("FROM_SEND")
            && text.matches("READY").count() >= 2
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let _ = fs::remove_file(&start_path);
    handle.send_input_bytes(b"exit\n");

    assert_eq!(
        panes.pane_count(),
        2,
        "split-window should add a pane; snapshot:\n{text}"
    );
    assert!(
        text.matches("READY").count() >= 2,
        "capture-pane should print the captured READY line; snapshot:\n{text}"
    );
    assert!(
        text.contains("FROM_SEND"),
        "send-keys should queue input for the shell; snapshot:\n{text}"
    );
}

#[test]
fn terminal_pane_ipc_splits_and_selects_panes_by_geometry() {
    let socket_path = temp_socket_path("split-select");
    let group = TerminalPaneGroup::new(TerminalEmulator::new());
    let panes = group.handle();
    let first_pane_id = panes.active_pane_id().expect("active pane").raw();
    let mut desktop = desktop_with_group(group);
    draw_desktop(&mut desktop);

    let mut server = IpcServer::bind(&socket_path).expect("bind ipc");
    server.set_method_handler(terminal_pane_ipc_handler(TerminalPaneIpc::new(
        panes.clone(),
    )));

    let split =
        ProtocolRequest::split_window("split", Some(first_pane_id), PaneSplitDirection::Vertical);
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), split),
    );
    let new_pane_id = match response.result {
        Some(ProtocolResult::SplitWindow(result)) => {
            assert_eq!(result.pane_id, first_pane_id);
            assert_eq!(result.pane_count, 2);
            result.new_pane_id
        }
        other => panic!("expected split_window result, got {other:?}"),
    };
    let list = ProtocolRequest::list_panes("list-after-split");
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), list),
    );
    match response.result {
        Some(ProtocolResult::ListPanes(panes)) => {
            assert_eq!(panes.len(), 2);
            assert!(
                panes
                    .iter()
                    .any(|pane| pane.pane_id == new_pane_id && pane.is_active)
            );
            assert!(panes.iter().all(|pane| pane.rect.is_some()));
        }
        other => panic!("expected list_panes result, got {other:?}"),
    }

    let select_left =
        ProtocolRequest::select_pane("select-left", Some(new_pane_id), PaneSelectDirection::Left);
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), select_left),
    );
    match response.result {
        Some(ProtocolResult::SelectPane(result)) => {
            assert_eq!(result.previous_pane_id, new_pane_id);
            assert_eq!(result.pane_id, first_pane_id);
        }
        other => panic!("expected select_pane result, got {other:?}"),
    }

    let select_right = ProtocolRequest::select_pane(
        "select-right",
        Some(first_pane_id),
        PaneSelectDirection::Right,
    );
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), select_right),
    );
    match response.result {
        Some(ProtocolResult::SelectPane(result)) => {
            assert_eq!(result.previous_pane_id, first_pane_id);
            assert_eq!(result.pane_id, new_pane_id);
        }
        other => panic!("expected select_pane result, got {other:?}"),
    }

    let split_down = ProtocolRequest::split_window(
        "split-down",
        Some(first_pane_id),
        PaneSplitDirection::Horizontal,
    );
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), split_down),
    );
    let lower_pane_id = match response.result {
        Some(ProtocolResult::SplitWindow(result)) => {
            assert_eq!(result.pane_id, first_pane_id);
            assert_eq!(result.pane_count, 3);
            result.new_pane_id
        }
        other => panic!("expected split_window result, got {other:?}"),
    };

    let select_up =
        ProtocolRequest::select_pane("select-up", Some(lower_pane_id), PaneSelectDirection::Up);
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), select_up),
    );
    match response.result {
        Some(ProtocolResult::SelectPane(result)) => {
            assert_eq!(result.previous_pane_id, lower_pane_id);
            assert_eq!(result.pane_id, first_pane_id);
        }
        other => panic!("expected select_pane result, got {other:?}"),
    }

    let select_down = ProtocolRequest::select_pane(
        "select-down",
        Some(first_pane_id),
        PaneSelectDirection::Down,
    );
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path, select_down),
    );
    match response.result {
        Some(ProtocolResult::SelectPane(result)) => {
            assert_eq!(result.previous_pane_id, first_pane_id);
            assert_eq!(result.pane_id, lower_pane_id);
        }
        other => panic!("expected select_pane result, got {other:?}"),
    }
}

#[test]
fn terminal_pane_ipc_breaks_pane_into_window_and_displays_popup_window() {
    let socket_path = temp_socket_path("break-popup");
    let group = TerminalPaneGroup::new(TerminalEmulator::new());
    let panes = group.handle();
    let first_pane_id = panes.active_pane_id().expect("active pane").raw();
    let mut desktop = desktop_with_group(group);
    draw_desktop(&mut desktop);

    let mut server = IpcServer::bind(&socket_path).expect("bind ipc");
    server.set_method_handler(terminal_pane_ipc_handler(TerminalPaneIpc::new(
        panes.clone(),
    )));

    let split = ProtocolRequest::split_window(
        "split-before-break",
        Some(first_pane_id),
        PaneSplitDirection::Vertical,
    );
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), split),
    );
    let pane_to_break = match response.result {
        Some(ProtocolResult::SplitWindow(result)) => result.new_pane_id,
        other => panic!("expected split_window result, got {other:?}"),
    };
    let break_pane = ProtocolRequest::break_pane("break", pane_to_break);
    let response = drive_until_response(
        &mut server,
        &mut desktop,
        spawn_request(socket_path.clone(), break_pane),
    );
    let detached_window_id = match response.result {
        Some(ProtocolResult::BreakPane(result)) => {
            assert_eq!(result.pane_id, pane_to_break);
            assert_eq!(result.remaining_pane_count, 1);
            result.window_id
        }
        other => panic!("expected break_pane result, got {other:?}"),
    };
    assert_eq!(panes.pane_count(), 1);
    assert!(
        desktop
            .wm
            .window(WindowId::from_raw(detached_window_id))
            .is_some(),
        "detached pane should be hosted in an independent window"
    );

    let popup = ProtocolRequest::display_popup(
        "popup",
        Some("Popup".to_string()),
        Some(atto_ui::runtime::Rect {
            x: 10,
            y: 5,
            width: 30,
            height: 8,
        }),
        None,
    );
    let response =
        drive_until_response(&mut server, &mut desktop, spawn_request(socket_path, popup));
    let popup_window_id = match response.result {
        Some(ProtocolResult::DisplayPopup(result)) => result.window_id,
        other => panic!("expected display_popup result, got {other:?}"),
    };
    let popup = desktop
        .wm
        .window(WindowId::from_raw(popup_window_id))
        .expect("popup window");
    assert_eq!(popup.kind, WindowKind::Floating);
}
