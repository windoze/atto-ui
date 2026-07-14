use std::env;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::ipc::{IpcServer, send_protocol_request};
use atto_ui::protocol::{ProtocolRequest, ProtocolResponse, ProtocolResult};
use atto_ui::theme::Theme;
use atto_ui_terminal::{
    TerminalEmulator, TerminalPaneGroup, TerminalPaneIpc, terminal_pane_ipc_handler,
};
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
