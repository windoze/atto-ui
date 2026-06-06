use std::thread;
use std::time::Duration;

use atto_ui_test_host::PtyTestHost;

#[test]
fn chat_input_modes_switch() -> anyhow::Result<()> {
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
fn chat_load_more_on_scroll_top() -> anyhow::Result<()> {
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
