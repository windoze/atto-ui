use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};
use unicode_width::UnicodeWidthStr;

static CHAT_PTY_LOCK: Mutex<()> = Mutex::new(());

fn chat_pty_lock() -> MutexGuard<'static, ()> {
    CHAT_PTY_LOCK.lock().expect("chat PTY lock poisoned")
}

fn assert_text_absent_for(host: &PtyTestHost, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if screen.contains(needle) {
            panic!("expected text {needle:?} to remain absent.\n--- screen ---\n{screen}");
        }
        thread::sleep(Duration::from_millis(10));
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

fn find_close_button_on_title_row(host: &PtyTestHost, title: &str) -> Option<(u16, u16)> {
    let contents = host.screen_contents().ok()?;
    contents.lines().enumerate().find_map(|(y, line)| {
        line.find(title)?;
        let close_idx = line.find("[■]")?;
        let glyph_idx = close_idx.saturating_add("[".len());
        Some((
            line[..glyph_idx].width().min(u16::MAX as usize) as u16,
            y.min(u16::MAX as usize) as u16,
        ))
    })
}

#[test]
fn chat_input_modes_switch() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("Message", Duration::from_secs(2))?;

    host.send_str("c")?;
    host.wait_for_text("请选择一种回应方式", Duration::from_secs(2))?;
    host.wait_for_text("简短回复", Duration::from_secs(2))?;

    host.send_str("f")?;
    host.wait_for_text("是否继续执行?", Duration::from_secs(2))?;
    host.wait_for_text("继续", Duration::from_secs(2))?;

    host.send_str("t")?;
    host.wait_for_text("Message", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_input_modes_submit_callbacks() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("Message", Duration::from_secs(2))?;

    host.send_str("hello")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=hello", Duration::from_secs(2))?;

    host.send_str("c")?;
    host.wait_for_text("请选择一种回应方式", Duration::from_secs(2))?;
    host.key_with_mods(KeyCode::Down, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Tab, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: choice index=1", Duration::from_secs(2))?;
    host.wait_for_text("label=详细解释", Duration::from_secs(2))?;

    host.send_str("f")?;
    host.wait_for_text("是否继续执行?", Duration::from_secs(2))?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: choice index=0", Duration::from_secs(2))?;
    host.wait_for_text("label=继续", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_textarea_multiline_history_and_kill_ring() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 26)?;

    host.wait_for_text("Message", Duration::from_secs(2))?;

    host.send_str("line1")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::SHIFT)?;
    host.send_str("line2")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot.iter().any(|line| line.contains("line1"))
                && snapshot.iter().any(|line| line.contains("line2"))
        },
        Duration::from_secs(2),
    )?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=line1", Duration::from_secs(2))?;
    host.wait_for_text("line2", Duration::from_secs(2))?;

    host.send_str("uno")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=uno", Duration::from_secs(2))?;
    host.send_str("zwo")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=zwo", Duration::from_secs(2))?;

    host.key_with_mods(KeyCode::Up, KeyModifiers::NONE)?;
    host.send_str("-zz")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=zwo-zz", Duration::from_secs(2))?;

    host.key_with_mods(KeyCode::Up, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Up, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Down, KeyModifiers::NONE)?;
    host.send_str("-yy")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=zwo-zz-yy", Duration::from_secs(2))?;

    host.send_str("killme")?;
    host.send_ctrl('u')?;
    host.send_ctrl('y')?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=killme", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_auto_follow_pauses_after_user_scrolls_up() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("MSG-00", Duration::from_secs(2))?;

    host.send_str("a")?;
    host.wait_for_text("FOLLOW-1", Duration::from_secs(2))?;

    for _ in 0..10 {
        host.wheel_up(6, 6)?;
        thread::sleep(Duration::from_millis(30));
    }
    host.wait_for_text("MSG-15", Duration::from_secs(2))?;

    host.send_str("b")?;
    assert_text_absent_for(&host, "FOLLOW-2", Duration::from_millis(250));

    for _ in 0..30 {
        host.wheel_down(6, 6)?;
        thread::sleep(Duration::from_millis(30));
    }
    host.wait_for_text("FOLLOW-2", Duration::from_secs(2))?;

    host.send_str("d")?;
    host.wait_for_text("FOLLOW-3", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_load_more_on_scroll_top() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("MSG-00", Duration::from_secs(2))?;

    for _ in 0..6 {
        host.wheel_down(6, 6)?;
        thread::sleep(Duration::from_millis(60));
    }

    for _ in 0..12 {
        host.wheel_up(6, 6)?;
        thread::sleep(Duration::from_millis(60));
    }

    host.wait_for_text("HISTORY-1-0", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_streaming_markdown_tolerates_incomplete_blocks() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--streaming-markdown"], 80, 24)?;

    host.wait_for_text("STREAMING-MARKDOWN", Duration::from_secs(2))?;

    host.send_str("1")?;
    host.wait_for_text("STREAMING-CODE", Duration::from_secs(2))?;
    host.wait_for_text("fn main()", Duration::from_secs(2))?;

    host.send_str("2")?;
    host.wait_for_text("STREAMING-CODE", Duration::from_secs(2))?;

    host.send_str("3")?;
    host.wait_for_text("| half |", Duration::from_secs(2))?;
    let partial = host.screen_snapshot()?;
    assert!(partial.iter().any(|line| line.contains("| Name | Value |")));
    assert!(partial.iter().all(|line| !line.contains('┬')));

    host.send_str("4")?;
    host.wait_for_text("stable", Duration::from_secs(2))?;
    let complete = host.screen_snapshot()?;
    assert!(complete.iter().any(|line| line.contains('┬')));

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_streaming_delta_append_renders_accumulated_text() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--streaming-markdown"], 80, 24)?;

    host.wait_for_text("STREAMING-MARKDOWN", Duration::from_secs(2))?;

    host.send_str("5")?;
    host.wait_for_text("STREAM-DELTA-A", Duration::from_secs(2))?;

    host.send_str("6")?;
    host.wait_for_text("STREAM-DELTA-A + STREAM-DELTA-B", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_tool_call_disclosure_streams_status_and_toggles() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--tool-call"], 80, 24)?;

    host.wait_for_text("Tool: build", Duration::from_secs(2))?;
    host.wait_for_text("[~]", Duration::from_secs(2))?;
    host.wait_for_text("TOOL-START", Duration::from_secs(2))?;

    host.send_str("1")?;
    host.wait_for_text("TOOL-OUTPUT-1", Duration::from_secs(2))?;

    host.send_str("2")?;
    host.wait_for_text("TOOL-OUTPUT-2", Duration::from_secs(2))?;

    host.send_str("3")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("[x]") && line.contains("Tool: build"))
        },
        Duration::from_secs(2),
    )?;

    let (x, y) = find_text_position(&host, "Tool: build").expect("tool header position");
    host.click(x, y)?;
    host.wait_for_screen(
        |snapshot| snapshot.iter().all(|line| !line.contains("TOOL-OUTPUT-1")),
        Duration::from_secs(2),
    )?;

    host.click(x, y)?;
    host.wait_for_text("TOOL-OUTPUT-2", Duration::from_secs(2))?;

    host.send_str("4")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("[!]") && line.contains("Tool: build"))
        },
        Duration::from_secs(2),
    )?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_artifact_code_link_opens_text_viewer_window() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--artifact-link"], 100, 28)?;

    host.wait_for_text("Artifact Code: main.rs", Duration::from_secs(2))?;
    let (x, y) = find_text_position(&host, "Artifact Code: main.rs").expect("code link");
    host.click(x, y)?;

    host.wait_for_text("Code: main.rs", Duration::from_secs(2))?;
    host.wait_for_text("Code Artifact: main.rs", Duration::from_secs(2))?;
    host.wait_for_text("CODE-ARTIFACT", Duration::from_secs(2))?;

    let (close_x, close_y) =
        find_close_button_on_title_row(&host, "Code: main.rs").expect("code viewer close button");
    host.click(close_x, close_y)?;
    host.wait_for_screen(
        |snapshot| !snapshot.iter().any(|line| line.contains("CODE-ARTIFACT")),
        Duration::from_secs(2),
    )?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_artifact_diff_link_opens_colored_diff_viewer_window() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--artifact-link"], 100, 28)?;

    host.wait_for_text("Artifact Diff: main.patch", Duration::from_secs(2))?;
    let (x, y) = find_text_position(&host, "Artifact Diff: main.patch").expect("diff link");
    host.click(x, y)?;

    host.wait_for_text("Diff: main.patch", Duration::from_secs(2))?;
    host.wait_for_text("Diff Artifact: main.patch", Duration::from_secs(2))?;
    host.wait_for_text("+    println!(\"DIFF-ARTIFACT\");", Duration::from_secs(2))?;
    host.wait_for_text("-    println!(\"old\");", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}
