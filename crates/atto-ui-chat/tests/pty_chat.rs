use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};

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
