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
