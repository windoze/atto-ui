#![forbid(unsafe_code)]

use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};

static AGENT_PTY_LOCK: Mutex<()> = Mutex::new(());
const PTY_WAIT: Duration = Duration::from_secs(5);

fn agent_pty_lock() -> MutexGuard<'static, ()> {
    AGENT_PTY_LOCK.lock().expect("agent PTY lock poisoned")
}

fn spawn_agent() -> anyhow::Result<PtyTestHost> {
    let bin = env!("CARGO_BIN_EXE_snapshot_agent_app");
    PtyTestHost::spawn(bin, &[], 100, 32)
}

fn submit_text(host: &mut PtyTestHost, text: &str) -> anyhow::Result<()> {
    host.send_str(text)?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)
}

fn assert_text_absent_for(host: &PtyTestHost, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if screen.contains(needle) {
            panic!("expected text {needle:?} to remain absent.\n--- screen ---\n{screen}");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn agent_mock_fixture_streams_submitted_input() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;
    host.wait_for_text("provider: mock", PTY_WAIT)?;
    host.wait_for_text("model: deepseek-chat", PTY_WAIT)?;
    host.wait_for_text("plan: auto", PTY_WAIT)?;

    submit_text(&mut host, "hello agent")?;

    host.wait_for_text("hello agent", PTY_WAIT)?;
    host.wait_for_text("streaming", PTY_WAIT)?;
    host.wait_for_text("Mock assistant: hello agent", PTY_WAIT)?;
    host.wait_for_text("Done.", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_slash_commands_render_outputs_and_update_state() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(&mut host, "/help")?;
    host.wait_for_text("Available commands:", PTY_WAIT)?;
    host.wait_for_text("/abort: Cancel the active mock turn.", PTY_WAIT)?;

    submit_text(&mut host, "/plan")?;
    host.wait_for_text("Plan mode set to off.", PTY_WAIT)?;
    host.wait_for_text("plan: off", PTY_WAIT)?;

    submit_text(&mut host, "/skills")?;
    host.wait_for_text("Skills: none registered yet.", PTY_WAIT)?;

    submit_text(&mut host, "/tools")?;
    host.wait_for_text("Tools: none registered yet.", PTY_WAIT)?;

    submit_text(&mut host, "/abort")?;
    host.wait_for_text("No active turn to abort.", PTY_WAIT)?;

    submit_text(&mut host, "/clear")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .all(|line| !line.contains("Available commands:"))
                && snapshot
                    .iter()
                    .all(|line| !line.contains("No active turn to abort."))
        },
        PTY_WAIT,
    )?;
    host.wait_for_text("plan: off", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_escape_cancels_active_mock_turn_without_late_done() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(&mut host, "cancel fixture")?;
    host.wait_for_text("streaming", PTY_WAIT)?;
    host.key_with_mods(KeyCode::Esc, KeyModifiers::NONE)?;

    host.wait_for_text("canceled", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;
    assert_text_absent_for(&host, "Done.", Duration::from_millis(700));

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
