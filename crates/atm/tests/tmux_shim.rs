//! Integration tests for the `tmux` shim binary shipped by `atm`.
//!
//! These were lifted out of `atto-ui-terminal` when the `tmux` shim moved into
//! the `atm` crate. `tmux_shim_from_child_path_drives_native_pane_methods`
//! exercises the end-to-end loop: a real `TerminalPaneGroup` + IPC server (from
//! `atto-ui-terminal`) driven through the `tmux` shim that lives here. Helpers
//! are duplicated from `atto-ui-terminal`'s test suite on purpose — they are
//! small, test-only, and keep this crate's tests self-contained.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_terminal::{
    TerminalEmulator, TerminalHandle, TerminalPaneGroup, TerminalTmuxEnvironmentConfig,
    terminal_pane_ipc_handler,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

static NEXT_SOCKET_ID: AtomicUsize = AtomicUsize::new(0);

fn temp_socket_path(test_name: &str) -> PathBuf {
    let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);
    env::temp_dir().join(format!("atm-{test_name}-{}-{id}.sock", std::process::id()))
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

fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn tmux_shim_dir() -> String {
    Path::new(env!("CARGO_BIN_EXE_tmux"))
        .parent()
        .expect("tmux shim bin directory")
        .to_string_lossy()
        .into_owned()
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

    let mut server = atto_ui::ipc::IpcServer::bind(&socket_path).expect("bind ipc");
    server.set_method_handler(terminal_pane_ipc_handler(
        atto_ui_terminal::TerminalPaneIpc::new(panes.clone()),
    ));
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
