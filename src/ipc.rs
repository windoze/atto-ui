//! Unix socket transport for the scripting control plane.
//!
//! This module deliberately keeps transport concerns separate from
//! `DesktopInspector`: socket threads deserialize protocol requests and enqueue
//! them, while the UI thread drains the queue and executes the inspector calls.

use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use serde_json::Value;

use crate::ComponentError;
use crate::app::Desktop;
use crate::inspect::{DesktopInspector, WaitResult};
use crate::protocol::{
    ProtocolId, ProtocolMethod, ProtocolRequest, ProtocolResponse, ProtocolResult, WaitForParams,
};
use crate::runtime::Rect as ProtocolRect;

/// Environment variable that points clients at the active atto-ui IPC socket.
pub const IPC_SOCKET_ENV: &str = "ATTO_UI_SOCKET";

const ACCEPT_BACKOFF: Duration = Duration::from_millis(10);
const DEFAULT_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Configuration for binding an IPC server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpcServerConfig {
    socket_path: PathBuf,
}

impl IpcServerConfig {
    /// Creates a config with an explicit Unix socket path.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Reads the standard IPC socket environment variable.
    pub fn from_env() -> Option<Self> {
        env::var_os(IPC_SOCKET_ENV).map(Self::new)
    }

    /// Returns the configured Unix socket path.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

struct UiRequest {
    request: ProtocolRequest,
    response_tx: mpsc::Sender<ProtocolResponse>,
}

struct PendingWait {
    id: ProtocolId,
    screen: Rect,
    condition: crate::WaitCondition,
    deadline: Instant,
    poll_interval: Duration,
    next_poll: Instant,
    polls: u64,
    response_tx: mpsc::Sender<ProtocolResponse>,
}

/// Running IPC server handle owned by the UI thread.
pub struct IpcServer {
    socket_path: PathBuf,
    request_rx: mpsc::Receiver<UiRequest>,
    shutdown: Arc<AtomicBool>,
    listener_thread: Option<JoinHandle<()>>,
    pending_waits: Vec<PendingWait>,
}

impl IpcServer {
    /// Binds a Unix socket and starts the accept thread.
    pub fn bind(socket_path: impl Into<PathBuf>) -> io::Result<Self> {
        Self::from_config(IpcServerConfig::new(socket_path))
    }

    /// Binds from `ATTO_UI_SOCKET` when it is present.
    pub fn from_env() -> io::Result<Option<Self>> {
        IpcServerConfig::from_env()
            .map(Self::from_config)
            .transpose()
    }

    /// Binds a Unix socket using a prepared config.
    pub fn from_config(config: IpcServerConfig) -> io::Result<Self> {
        let socket_path = config.socket_path;
        let listener = bind_unix_listener(&socket_path)?;
        listener.set_nonblocking(true)?;

        let (request_tx, request_rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let listener_shutdown = Arc::clone(&shutdown);
        let listener_thread = thread::spawn(move || {
            accept_loop(listener, request_tx, listener_shutdown);
        });

        Ok(Self {
            socket_path,
            request_rx,
            shutdown,
            listener_thread: Some(listener_thread),
            pending_waits: Vec::new(),
        })
    }

    /// Returns the socket path this server is listening on.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Drains queued requests on the UI thread and sends protocol responses.
    pub fn drain_pending(&mut self, desktop: &mut Desktop, screen: Rect) {
        while let Ok(request) = self.request_rx.try_recv() {
            self.dispatch_request(desktop, screen, request);
        }
        self.poll_pending_waits(desktop);
    }

    fn dispatch_request(&mut self, desktop: &mut Desktop, fallback_screen: Rect, ui: UiRequest) {
        let ProtocolRequest { id, method } = ui.request;
        match method {
            ProtocolMethod::WaitFor(params) => {
                self.start_wait(desktop, fallback_screen, id, params, ui.response_tx);
            }
            method => {
                let response = execute_immediate_method(desktop, id, method);
                let _ = ui.response_tx.send(response);
            }
        }
    }

    fn start_wait(
        &mut self,
        desktop: &mut Desktop,
        _fallback_screen: Rect,
        id: ProtocolId,
        params: WaitForParams,
        response_tx: mpsc::Sender<ProtocolResponse>,
    ) {
        let timeout = Duration::from_millis(params.timeout_ms);
        let poll_interval = params
            .poll_interval_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_WAIT_POLL_INTERVAL);
        let now = Instant::now();
        let deadline = now.checked_add(timeout).unwrap_or(now);
        let Some(screen) = protocol_rect(params.screen) else {
            let response = ProtocolResponse::error(
                id,
                ComponentError::invalid_value("screen", "non-empty screen rect"),
            );
            let _ = response_tx.send(response);
            return;
        };

        let mut wait = PendingWait {
            id,
            screen,
            condition: params.condition,
            deadline,
            poll_interval,
            next_poll: now,
            polls: 0,
            response_tx,
        };

        if let Some(response) = poll_wait_once(desktop, &mut wait) {
            let _ = wait.response_tx.send(response);
        } else {
            wait.next_poll = instant_after(Instant::now(), wait.poll_interval);
            self.pending_waits.push(wait);
        }
    }

    fn poll_pending_waits(&mut self, desktop: &mut Desktop) {
        let mut pending = Vec::with_capacity(self.pending_waits.len());
        for mut wait in self.pending_waits.drain(..) {
            let now = Instant::now();
            if now < wait.next_poll && now < wait.deadline {
                pending.push(wait);
                continue;
            }

            if let Some(response) = poll_wait_once(desktop, &mut wait) {
                let _ = wait.response_tx.send(response);
            } else {
                wait.next_poll = instant_after(Instant::now(), wait.poll_interval);
                pending.push(wait);
            }
        }
        self.pending_waits = pending;
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(thread) = self.listener_thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.socket_path);
    }
}

/// Sends one protocol request to a socket and reads one response.
pub fn send_protocol_request(
    socket_path: impl AsRef<Path>,
    request: &ProtocolRequest,
) -> io::Result<ProtocolResponse> {
    let mut stream = UnixStream::connect(socket_path)?;
    write_json_line(&mut stream, request)?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "IPC server closed before sending a response",
        ));
    }
    serde_json::from_str(line.trim_end()).map_err(json_error)
}

fn bind_unix_listener(socket_path: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(socket_path) {
        Ok(listener) => Ok(listener),
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => {
            remove_stale_socket(socket_path, &err)?;
            UnixListener::bind(socket_path)
        }
        Err(err) => Err(err),
    }
}

fn remove_stale_socket(socket_path: &Path, original: &io::Error) -> io::Result<()> {
    let metadata = fs::metadata(socket_path)?;
    if !metadata.file_type().is_socket() {
        return Err(io::Error::new(
            original.kind(),
            format!(
                "{} already exists and is not a Unix socket",
                socket_path.display()
            ),
        ));
    }
    if UnixStream::connect(socket_path).is_ok() {
        return Err(io::Error::new(
            original.kind(),
            format!("{} already has a listening server", socket_path.display()),
        ));
    }
    fs::remove_file(socket_path)
}

fn accept_loop(
    listener: UnixListener,
    request_tx: mpsc::Sender<UiRequest>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _addr)) => {
                let tx = request_tx.clone();
                thread::spawn(move || handle_client(stream, tx));
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_BACKOFF);
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

fn handle_client(mut stream: UnixStream, request_tx: mpsc::Sender<UiRequest>) {
    let read_stream = match stream.try_clone() {
        Ok(read_stream) => read_stream,
        Err(err) => {
            let response = protocol_parse_error(ProtocolId::from("invalid"), err);
            let _ = write_response(&mut stream, &response);
            return;
        }
    };
    let mut reader = BufReader::new(read_stream);

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) if line.trim().is_empty() => continue,
            Ok(_) => {
                let id = protocol_id_from_json_line(&line);
                let request = match serde_json::from_str::<ProtocolRequest>(line.trim_end()) {
                    Ok(request) => request,
                    Err(err) => {
                        let response = protocol_parse_error(id, err);
                        let _ = write_response(&mut stream, &response);
                        continue;
                    }
                };

                let (response_tx, response_rx) = mpsc::channel();
                if request_tx
                    .send(UiRequest {
                        request,
                        response_tx,
                    })
                    .is_err()
                {
                    let response = ProtocolResponse::error(
                        id,
                        ComponentError::render_failed("IPC request queue is closed"),
                    );
                    let _ = write_response(&mut stream, &response);
                    break;
                }

                match response_rx.recv() {
                    Ok(response) => {
                        if write_response(&mut stream, &response).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let response = ProtocolResponse::error(
                            id,
                            ComponentError::render_failed("IPC response channel is closed"),
                        );
                        let _ = write_response(&mut stream, &response);
                        break;
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

fn write_response(stream: &mut UnixStream, response: &ProtocolResponse) -> io::Result<()> {
    write_json_line(stream, response)?;
    stream.flush()
}

fn write_json_line<T: serde::Serialize>(stream: &mut UnixStream, value: &T) -> io::Result<()> {
    serde_json::to_writer(&mut *stream, value).map_err(json_error)?;
    stream.write_all(b"\n")
}

fn execute_immediate_method(
    desktop: &mut Desktop,
    id: ProtocolId,
    method: ProtocolMethod,
) -> ProtocolResponse {
    let result = match method {
        ProtocolMethod::Query(params) => DesktopInspector::new(desktop)
            .query(params.target, &params.property)
            .map(ProtocolResult::Query),
        ProtocolMethod::Invoke(params) => {
            let screen = protocol_rect(params.screen)
                .ok_or_else(|| ComponentError::invalid_value("screen", "non-empty screen rect"));
            screen.and_then(|screen| {
                DesktopInspector::new(desktop)
                    .invoke(screen, params.target, params.action)
                    .map(ProtocolResult::Invoke)
            })
        }
        ProtocolMethod::Tree(params) => {
            let screen = protocol_rect(params.screen)
                .ok_or_else(|| ComponentError::invalid_value("screen", "non-empty screen rect"));
            screen.and_then(|screen| {
                DesktopInspector::new(desktop)
                    .export_snapshot(screen)
                    .map(ProtocolResult::Tree)
            })
        }
        ProtocolMethod::PropertyNames(params) => DesktopInspector::new(desktop)
            .property_names(&params.id)
            .map(ProtocolResult::PropertyNames),
        ProtocolMethod::WaitFor(_) => Err(ComponentError::render_failed(
            "wait_for must be scheduled as a pending IPC request",
        )),
    };

    match result {
        Ok(result) => ProtocolResponse::success(id, result),
        Err(err) => ProtocolResponse::error(id, err),
    }
}

fn poll_wait_once(desktop: &mut Desktop, wait: &mut PendingWait) -> Option<ProtocolResponse> {
    wait.polls += 1;
    let poll = DesktopInspector::new(desktop).poll_wait_condition(wait.screen, &wait.condition);
    match poll {
        Ok(Some(value)) => Some(ProtocolResponse::success(
            wait.id.clone(),
            ProtocolResult::WaitFor(WaitResult {
                polls: wait.polls,
                value: Some(value),
            }),
        )),
        Ok(None) => {
            if Instant::now() >= wait.deadline {
                Some(ProtocolResponse::error(
                    wait.id.clone(),
                    ComponentError::timeout(format!(
                        "condition not met after {} polls: {:?}",
                        wait.polls, wait.condition
                    )),
                ))
            } else {
                None
            }
        }
        Err(err) => Some(ProtocolResponse::error(wait.id.clone(), err)),
    }
}

fn instant_after(now: Instant, duration: Duration) -> Instant {
    now.checked_add(duration).unwrap_or(now)
}

fn protocol_rect(rect: ProtocolRect) -> Option<Rect> {
    if rect.width == 0 || rect.height == 0 {
        return None;
    }
    Some(Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    })
}

fn protocol_id_from_json_line(line: &str) -> ProtocolId {
    serde_json::from_str::<Value>(line.trim_end())
        .ok()
        .and_then(|value| match value.get("id") {
            Some(Value::String(value)) => Some(ProtocolId::String(value.clone())),
            Some(Value::Number(value)) => value.as_u64().map(ProtocolId::Number),
            _ => None,
        })
        .unwrap_or_else(|| ProtocolId::from("invalid"))
}

fn protocol_parse_error(id: ProtocolId, err: impl ToString) -> ProtocolResponse {
    ProtocolResponse::error(
        id,
        ComponentError::invalid_value(
            "request",
            format!("valid protocol request JSON ({})", err.to_string()),
        ),
    )
}

fn json_error(err: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppControl, AppHost, MenuBar};
    use crate::composable::ComponentTagExt;
    use crate::reactive::Binding;
    use crate::theme::Theme;
    use crate::widgets::Checkbox;
    use crate::wm::{Window, WindowKind};
    use crate::{ComponentCommand, ComponentTarget, ComponentValue};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_SOCKET_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_socket_path(test_name: &str) -> PathBuf {
        let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);
        env::temp_dir().join(format!(
            "atto-ui-{test_name}-{}-{id}.sock",
            std::process::id()
        ))
    }

    fn screen() -> Rect {
        Rect::new(0, 0, 80, 24)
    }

    fn protocol_screen() -> ProtocolRect {
        ProtocolRect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        }
    }

    fn drive_until_response(
        host: &mut AppHost,
        rx: mpsc::Receiver<io::Result<ProtocolResponse>>,
    ) -> ProtocolResponse {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            host.step().expect("host step");
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

    fn send_raw_protocol_line(path: &Path, line: &str) -> io::Result<ProtocolResponse> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.write_all(line.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        reader.read_line(&mut response)?;
        serde_json::from_str(response.trim_end()).map_err(json_error)
    }

    #[test]
    fn ipc_server_queries_and_invokes_on_ui_thread() {
        let socket_path = temp_socket_path("query-invoke");
        let checked = Binding::new(false);
        let checked_for_window = checked.clone();
        let mut host = AppHost::new_headless(screen(), move |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "IPC",
                    Rect::new(2, 2, 24, 6),
                    Box::new(Checkbox::new("Flag", checked_for_window).tag("flag")),
                )
                .with_tag("ipc-window"),
                screen,
            );
            Ok(desktop)
        })
        .expect("host");
        host.enable_ipc(&socket_path).expect("enable ipc");

        let query = ProtocolRequest::query(1, ComponentTarget::Id("flag".to_string()), "checked");
        let response = drive_until_response(&mut host, spawn_request(socket_path.clone(), query));
        assert_eq!(
            response.result,
            Some(ProtocolResult::Query(ComponentValue::Bool(false)))
        );

        let invoke = ProtocolRequest::invoke(
            2,
            protocol_screen(),
            ComponentTarget::Id("flag".to_string()),
            ComponentCommand::Toggle,
        );
        let response = drive_until_response(&mut host, spawn_request(socket_path.clone(), invoke));
        assert!(response.is_success(), "invoke failed: {response:?}");
        assert!(checked.get());

        let query = ProtocolRequest::query(3, ComponentTarget::Id("flag".to_string()), "checked");
        let response = drive_until_response(&mut host, spawn_request(socket_path, query));
        assert_eq!(
            response.result,
            Some(ProtocolResult::Query(ComponentValue::Bool(true)))
        );
    }

    #[test]
    fn ipc_server_maps_boundary_failures_to_protocol_errors() {
        let socket_path = temp_socket_path("errors");
        let checked = Binding::new(false);
        let checked_for_window = checked.clone();
        let mut host = AppHost::new_headless(screen(), move |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "IPC",
                    Rect::new(2, 2, 24, 6),
                    Box::new(Checkbox::new("Flag", checked_for_window).tag("flag")),
                )
                .with_tag("ipc-window"),
                screen,
            );
            Ok(desktop)
        })
        .expect("host");
        host.enable_ipc(&socket_path).expect("enable ipc");

        let names = ProtocolRequest::property_names("names", "flag");
        let response = drive_until_response(&mut host, spawn_request(socket_path.clone(), names));
        match response.result {
            Some(ProtocolResult::PropertyNames(names)) => {
                assert!(names.iter().any(|name| name == "checked"));
            }
            other => panic!("expected property_names result, got {other:?}"),
        }

        let missing = ProtocolRequest::query(
            "missing",
            ComponentTarget::Id("missing".to_string()),
            "checked",
        );
        let response = drive_until_response(&mut host, spawn_request(socket_path.clone(), missing));
        assert_eq!(
            response.error,
            Some(ComponentError::NotFound("missing".to_string()))
        );

        let unsupported = ProtocolRequest::invoke(
            "unsupported",
            protocol_screen(),
            ComponentTarget::Id("flag".to_string()),
            ComponentCommand::Custom {
                name: "nope".to_string(),
                payload: Vec::new(),
            },
        );
        let response =
            drive_until_response(&mut host, spawn_request(socket_path.clone(), unsupported));
        assert_eq!(
            response.error,
            Some(ComponentError::ActionNotSupported("nope".to_string()))
        );

        let response = send_raw_protocol_line(
            &socket_path,
            r#"{"id":"bad-method","method":"unknown","params":{}}"#,
        )
        .expect("invalid method response");
        assert_eq!(response.id, ProtocolId::from("bad-method"));
        assert!(matches!(
            response.error,
            Some(ComponentError::InvalidValue { name, .. }) if name == "request"
        ));
    }

    #[test]
    fn ipc_wait_for_is_polled_without_blocking_other_requests() {
        let socket_path = temp_socket_path("wait-for");
        let ready = Binding::new(false);
        let ready_for_window = ready.clone();
        let mut host = AppHost::new_headless(screen(), move |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "IPC",
                    Rect::new(2, 2, 24, 6),
                    Box::new(Checkbox::new("Ready", ready_for_window).tag("ready")),
                )
                .with_tag("ipc-window"),
                screen,
            );
            Ok(desktop)
        })
        .expect("host");
        host.enable_ipc(&socket_path).expect("enable ipc");

        let wait = ProtocolRequest::wait_for(
            "wait",
            protocol_screen(),
            crate::WaitCondition::property_equals(
                ComponentTarget::Id("ready".to_string()),
                "checked",
                ComponentValue::Bool(true),
            ),
            1_000,
        );
        let wait_rx = spawn_request(socket_path.clone(), wait);

        let query =
            ProtocolRequest::query("query", ComponentTarget::Id("ready".to_string()), "checked");
        let query_response =
            drive_until_response(&mut host, spawn_request(socket_path.clone(), query));
        assert_eq!(
            query_response.result,
            Some(ProtocolResult::Query(ComponentValue::Bool(false)))
        );

        ready.set(true);
        let wait_response = drive_until_response(&mut host, wait_rx);
        match wait_response.result {
            Some(ProtocolResult::WaitFor(result)) => {
                assert!(result.polls >= 1);
                assert_eq!(result.value, Some(ComponentValue::Bool(true)));
            }
            other => panic!("expected wait_for result, got {other:?}"),
        }
    }

    #[test]
    fn ipc_focused_target_respects_modal_focus_boundary() {
        let socket_path = temp_socket_path("modal");
        let normal_checked = Binding::new(false);
        let modal_checked = Binding::new(false);
        let normal_for_window = normal_checked.clone();
        let modal_for_window = modal_checked.clone();
        let mut host = AppHost::new_headless(screen(), move |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Normal",
                    Rect::new(2, 2, 24, 6),
                    Box::new(Checkbox::new("Normal", normal_for_window).tag("normal-flag")),
                )
                .with_tag("normal-window"),
                screen,
            );
            desktop.add_window(
                Window::new(
                    WindowKind::Modal,
                    "Modal",
                    Rect::new(10, 4, 24, 6),
                    Box::new(Checkbox::new("Modal", modal_for_window).tag("modal-flag")),
                )
                .with_tag("modal-window"),
                screen,
            );
            Ok(desktop)
        })
        .expect("host");
        host.enable_ipc(&socket_path).expect("enable ipc");
        host.step().expect("initial layout");

        let invoke = ProtocolRequest::invoke(
            "focused",
            protocol_screen(),
            ComponentTarget::Focused,
            ComponentCommand::Toggle,
        );
        let response = drive_until_response(&mut host, spawn_request(socket_path, invoke));
        assert!(response.is_success(), "focused invoke failed: {response:?}");
        assert!(!normal_checked.get());
        assert!(modal_checked.get());
        assert_eq!(host.step().expect("post step"), AppControl::Continue);
    }
}
