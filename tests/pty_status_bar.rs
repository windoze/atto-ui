use std::time::Duration;

use atto_ui_test_host::PtyTestHost;

#[test]
fn pty_status_bar_aligns_unicode_right_text() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--status-unicode"], 40, 12)?;

    host.wait_for_text("状态栏", Duration::from_secs(2))?;
    host.wait_for_text("🦀", Duration::from_secs(2))?;

    let status_y = host.rows().saturating_sub(1);
    let crab_x = host.cols().saturating_sub(2);
    assert_eq!(
        host.cell_contents(crab_x, status_y)?,
        "🦀",
        "expected right status text to touch the right edge\n--- screen ---\n{}",
        host.screen_contents().unwrap_or_default()
    );

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn pty_status_bar_truncates_long_cjk_without_panic() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--status-long-cjk"], 5, 8)?;

    host.wait_for_text("你好", Duration::from_secs(2))?;

    let status_y = host.rows().saturating_sub(1);
    assert_eq!(host.cell_contents(0, status_y)?, "你");
    assert_eq!(host.cell_contents(2, status_y)?, "好");
    assert_eq!(host.cell_contents(4, status_y)?, " ");

    let screen = host.screen_contents()?;
    assert!(
        !screen.contains('�'),
        "expected no replacement character after truncating wide status text\n--- screen ---\n{screen}"
    );

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
