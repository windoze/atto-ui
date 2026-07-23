use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use atto_ui::app::{AppHost, Desktop, MenuBar};
use atto_ui::composable::ComponentTagExt;
use atto_ui::protocol::{ProtocolResponse, ProtocolResult};
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::widgets::Checkbox;
use atto_ui::wm::{Window, WindowKind};
use atto_ui::{ComponentValue, InvokeDispatch};
use ratatui::layout::Rect;

static NEXT_SOCKET_ID: AtomicUsize = AtomicUsize::new(0);

fn temp_socket_path(test_name: &str) -> PathBuf {
    let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);
    env::temp_dir().join(format!(
        "atto-ui-cli-{test_name}-{}-{id}.sock",
        std::process::id()
    ))
}

fn screen() -> Rect {
    Rect::new(0, 0, 80, 24)
}

fn run_cli(host: &mut AppHost, socket_path: &Path, args: &[&str]) -> Output {
    let bin = env!("CARGO_BIN_EXE_atto").to_string();
    let socket_path = socket_path.to_path_buf();
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let output = Command::new(bin)
            .arg("--socket")
            .arg(socket_path)
            .args(args)
            .output();
        tx.send(output).expect("send CLI output");
    });

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        host.step().expect("host step");
        if let Ok(output) = rx.try_recv() {
            return output.expect("run atto CLI");
        }
        assert!(Instant::now() < deadline, "timed out waiting for atto CLI");
        thread::sleep(Duration::from_millis(5));
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn decode_response(output: &Output) -> ProtocolResponse {
    serde_json::from_slice(&output.stdout).expect("decode protocol response JSON")
}

#[test]
fn atto_cli_query_invoke_and_tree_round_trip_over_ipc() -> Result<()> {
    let socket_path = temp_socket_path("round-trip");
    let checked = Binding::new(false);
    let checked_for_window = checked.clone();
    let mut host = AppHost::new_headless(screen(), move |screen| {
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
        desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "CLI",
                Rect::new(2, 2, 24, 6),
                Box::new(Checkbox::new("Flag", checked_for_window).tag("flag")),
            )
            .with_tag("cli-window"),
            screen,
        );
        Ok(desktop)
    })?;
    host.enable_ipc(&socket_path)?;
    host.step()?;

    let output = run_cli(&mut host, &socket_path, &["query", "flag", "checked"]);
    assert_success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "false");

    let output = run_cli(
        &mut host,
        &socket_path,
        &["--json", "invoke", "flag", "toggle"],
    );
    assert_success(&output);
    let response = decode_response(&output);
    match response.result {
        Some(ProtocolResult::Invoke(result)) => {
            assert_eq!(result.dispatch, InvokeDispatch::Semantic);
        }
        other => panic!("expected invoke result, got {other:?}"),
    }
    assert!(checked.get());

    let output = run_cli(
        &mut host,
        &socket_path,
        &["--json", "query", "flag", "checked"],
    );
    assert_success(&output);
    let response = decode_response(&output);
    assert_eq!(
        response.result,
        Some(ProtocolResult::Query(ComponentValue::Bool(true)))
    );

    let output = run_cli(&mut host, &socket_path, &["--json", "tree"]);
    assert_success(&output);
    let response = decode_response(&output);
    match response.result {
        Some(ProtocolResult::Tree(snapshot)) => {
            assert!(snapshot.tree.find_by_id("flag").is_some());
            assert!(snapshot.tree.find_by_id("cli-window").is_some());
        }
        other => panic!("expected tree result, got {other:?}"),
    }

    Ok(())
}
