#![forbid(unsafe_code)]

use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};
use unicode_width::UnicodeWidthStr;

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

fn find_text_position(host: &PtyTestHost, needle: &str) -> Option<(u16, u16)> {
    let contents = host.screen_contents().ok()?;
    contents.lines().enumerate().find_map(|(y, line)| {
        line.find(needle).map(|byte_idx| {
            let x = UnicodeWidthStr::width(&line[..byte_idx]);
            (
                x.min(u16::MAX as usize) as u16,
                y.min(u16::MAX as usize) as u16,
            )
        })
    })
}

fn click_text(host: &mut PtyTestHost, needle: &str) -> anyhow::Result<()> {
    let (x, y) = find_text_position(host, needle)
        .unwrap_or_else(|| panic!("expected clickable text {needle:?} to be visible"));
    host.click(x, y)
}

fn click_text_center(host: &mut PtyTestHost, needle: &str) -> anyhow::Result<()> {
    let (x, y) = find_text_position(host, needle)
        .unwrap_or_else(|| panic!("expected clickable text {needle:?} to be visible"));
    let offset = UnicodeWidthStr::width(needle) / 2;
    host.click(x.saturating_add(offset.min(u16::MAX as usize) as u16), y)
}

fn focus_message_input(host: &mut PtyTestHost) -> anyhow::Result<()> {
    let (x, y) = find_text_position(host, "Message")
        .unwrap_or_else(|| panic!("expected message input to be visible"));
    host.click(x.saturating_add(2), y.saturating_add(1))
}

fn scroll_chat_up(host: &mut PtyTestHost) -> anyhow::Result<()> {
    for _ in 0..8 {
        host.wheel_up(50, 15)?;
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn activate_message_action(host: &mut PtyTestHost, label: &str) -> anyhow::Result<()> {
    click_text_center(host, label)?;
    thread::sleep(Duration::from_millis(80));
    click_text_center(host, label)
}

#[test]
fn agent_mock_fixture_streams_submitted_input() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;
    host.wait_for_text("provider: mock", PTY_WAIT)?;
    host.wait_for_text("model: deepseek-chat", PTY_WAIT)?;
    host.wait_for_text("plan: auto", PTY_WAIT)?;
    host.wait_for_text("skills: 0", PTY_WAIT)?;

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
    host.wait_for_text("/abort: Cancel the active turn.", PTY_WAIT)?;

    submit_text(&mut host, "/plan")?;
    host.wait_for_text("Plan mode set to off.", PTY_WAIT)?;
    host.wait_for_text("plan: off", PTY_WAIT)?;

    submit_text(&mut host, "/skills")?;
    host.wait_for_text("Skills: 1 discovered, 0 loaded.", PTY_WAIT)?;
    host.wait_for_text("- [available] pty-fixture", PTY_WAIT)?;
    host.wait_for_text("skills: 0", PTY_WAIT)?;

    submit_text(&mut host, "/tools")?;
    host.wait_for_text("Tools: 5 registered.", PTY_WAIT)?;
    host.wait_for_text("apply_patch", PTY_WAIT)?;
    host.wait_for_text("read_file", PTY_WAIT)?;
    host.wait_for_text("list_files", PTY_WAIT)?;
    host.wait_for_text("run_command", PTY_WAIT)?;
    host.wait_for_text("search_text", PTY_WAIT)?;

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
fn agent_skill_commands_list_and_load_fixture_skill() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(&mut host, "/skills")?;
    host.wait_for_text("Skills: 1 discovered, 0 loaded.", PTY_WAIT)?;
    host.wait_for_text("- [available] pty-fixture", PTY_WAIT)?;
    host.wait_for_text("activate one.", PTY_WAIT)?;

    submit_text(&mut host, "/skill pty-fixture")?;
    host.wait_for_text("Loaded skill pty-fixture", PTY_WAIT)?;
    host.wait_for_text("skills: 1", PTY_WAIT)?;

    submit_text(&mut host, "/skills")?;
    host.wait_for_text("Skills: 1 discovered, 1 loaded.", PTY_WAIT)?;
    host.wait_for_text("- [loaded] pty-fixture", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_file_mention_context_probe_injects_workspace_file() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(
        &mut host,
        "agent-pty-context-probe @.atto/skills/pty-fixture/SKILL.md",
    )?;

    host.wait_for_text("Mock context probe:", PTY_WAIT)?;
    host.wait_for_text("<context_files>", PTY_WAIT)?;
    host.wait_for_text("name: pty-fixture", PTY_WAIT)?;
    host.wait_for_text(
        "Use this fixture to verify PTY skill slash commands.",
        PTY_WAIT,
    )?;
    host.wait_for_text("ready", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_snapshot_fixture_compacts_older_transcript() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(
        &mut host,
        "agent compact seed alpha beta gamma delta epsilon zeta eta theta iota kappa",
    )?;
    host.wait_for_text("Mock assistant: agent compact seed", PTY_WAIT)?;
    host.wait_for_text("Done.", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;

    submit_text(&mut host, "agent compact followup")?;

    host.wait_for_text("Mock assistant: agent compact followup", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;
    scroll_chat_up(&mut host)?;
    host.wait_for_text("Context compact: Complete", PTY_WAIT)?;
    host.wait_for_text("Local summary of 1 earlier transcript message", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_retry_and_edit_resubmit_restart_turns_from_pty() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(&mut host, "/plan off")?;
    host.wait_for_text("plan: off", PTY_WAIT)?;

    submit_text(&mut host, "agent-pty-retry-edit-seed")?;
    host.wait_for_text("Mock retry/edit turn 1:", PTY_WAIT)?;
    host.wait_for_text("agent-pty-retry-edit-seed", PTY_WAIT)?;
    host.wait_for_text("Done.", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;
    host.wait_for_text("Retry", PTY_WAIT)?;
    host.wait_for_text("Edit", PTY_WAIT)?;

    activate_message_action(&mut host, "Retry")?;
    host.wait_for_text("Mock retry/edit turn 2:", PTY_WAIT)?;
    host.wait_for_text("agent-pty-retry-edit-seed", PTY_WAIT)?;
    host.wait_for_text("Done.", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;

    activate_message_action(&mut host, "Edit")?;
    focus_message_input(&mut host)?;
    host.key_with_mods(KeyCode::End, KeyModifiers::NONE)?;
    host.send_str(" edited")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;

    host.wait_for_text("agent-pty-retry-edit-seed edited", PTY_WAIT)?;
    host.wait_for_text("Mock retry/edit turn 3:", PTY_WAIT)?;
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

#[test]
fn agent_mock_tool_result_renders_auto_allowed_read_file() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(&mut host, "agent-pty-read-file")?;

    host.wait_for_text("read_file", PTY_WAIT)?;
    host.wait_for_text("Tool result: call_read_cargo", PTY_WAIT)?;
    host.wait_for_text("Path: .atto/skills/pty-fixture/SKILL.md", PTY_WAIT)?;
    host.wait_for_text("name: pty-fixture", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_mock_tool_approval_allow_once_runs_command() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(&mut host, "/plan off")?;
    host.wait_for_text("Plan mode set to off.", PTY_WAIT)?;
    host.wait_for_text("plan: off", PTY_WAIT)?;

    submit_text(&mut host, "agent-pty-run-command")?;

    host.wait_for_text("run_command", PTY_WAIT)?;
    host.wait_for_text("Approval: Allow tool `run_command` to run?", PTY_WAIT)?;
    host.wait_for_text("Allow once", PTY_WAIT)?;
    host.wait_for_text("Allow project", PTY_WAIT)?;
    host.wait_for_text("Deny", PTY_WAIT)?;

    let (x, y) = find_text_position(&host, "Allow once").expect("allow once button");
    host.click(x, y)?;

    host.wait_for_text("[x] Allow once", PTY_WAIT)?;
    host.wait_for_text("Tool result: call_run_echo (exit 0)", PTY_WAIT)?;
    host.wait_for_text("AGENT-ALLOW-OUTPUT", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_mock_tool_approval_deny_writes_failed_result() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(&mut host, "/plan off")?;
    host.wait_for_text("Plan mode set to off.", PTY_WAIT)?;
    host.wait_for_text("plan: off", PTY_WAIT)?;

    submit_text(&mut host, "agent-pty-run-command")?;

    host.wait_for_text("Approval: Allow tool `run_command` to run?", PTY_WAIT)?;
    host.wait_for_text("Deny", PTY_WAIT)?;

    let (x, y) = find_text_position(&host, "Deny").expect("deny button");
    host.click(x, y)?;

    host.wait_for_text("[x] Deny", PTY_WAIT)?;
    host.wait_for_text("Tool result: call_run_echo", PTY_WAIT)?;
    host.wait_for_text(
        "User denied tool call run_command. The tool was not executed.",
        PTY_WAIT,
    )?;
    assert_text_absent_for(
        &host,
        "Tool result: call_run_echo (exit 0)",
        Duration::from_millis(300),
    );

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_plan_mode_generates_plan_and_accept_continues_execution() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;
    host.wait_for_text("plan: auto", PTY_WAIT)?;

    submit_text(&mut host, "Please update README.md with setup docs")?;

    host.wait_for_text("Plan: pending", PTY_WAIT)?;
    host.wait_for_text("Review the request and relevant context.", PTY_WAIT)?;
    host.wait_for_text(
        "Implement the requested change in the appropriate files.",
        PTY_WAIT,
    )?;
    host.wait_for_text("[ Accept ]", PTY_WAIT)?;
    host.wait_for_text("[ Reject ]", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;

    click_text(&mut host, "[ Accept ]")?;

    host.wait_for_text("The user accepted the plan.", PTY_WAIT)?;
    host.wait_for_text("streaming", PTY_WAIT)?;
    host.wait_for_text("Mock assistant:", PTY_WAIT)?;
    host.wait_for_text("Done.", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_plan_mode_reject_stops_without_execution() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;

    submit_text(&mut host, "Please update README.md with rejection docs")?;

    host.wait_for_text("Plan: pending", PTY_WAIT)?;
    host.wait_for_text("[ Reject ]", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;

    click_text(&mut host, "[ Reject ]")?;

    host.wait_for_text("[x] Rejected", PTY_WAIT)?;
    host.wait_for_text("ready", PTY_WAIT)?;
    assert_text_absent_for(&host, "Mock assistant:", Duration::from_millis(700));
    assert_text_absent_for(
        &host,
        "The user accepted the plan.",
        Duration::from_millis(700),
    );

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn agent_plan_mode_blocks_mutating_tool_before_acceptance() -> anyhow::Result<()> {
    let _guard = agent_pty_lock();
    let mut host = spawn_agent()?;

    host.wait_for_text("Atto Agent", PTY_WAIT)?;
    host.wait_for_text("plan: auto", PTY_WAIT)?;

    submit_text(&mut host, "agent-pty-run-command")?;

    host.wait_for_text("run_command", PTY_WAIT)?;
    host.wait_for_text("Tool result: call_run_echo", PTY_WAIT)?;
    host.wait_for_text(
        "Plan mode blocks mutating tools until the plan is accepted.",
        PTY_WAIT,
    )?;
    host.wait_for_text("ready", PTY_WAIT)?;
    assert_text_absent_for(&host, "Approval: Allow tool", Duration::from_millis(300));
    assert_text_absent_for(
        &host,
        "Tool result: call_run_echo (exit 0)",
        Duration::from_millis(300),
    );

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
