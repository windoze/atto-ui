use std::time::Duration;

use atto_ui::drawing::osc8_hyperlink_sequence;
use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};

#[test]
fn pty_t18_toast_windowing_and_multimodal() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--notifications-windowing-multimodal"], 80, 24)?;

    host.wait_for_text("Background task complete", Duration::from_secs(2))?;
    host.wait_for_text("Queued notification", Duration::from_secs(2))?;
    host.wait_for_screen(
        |rows| !rows.join("\n").contains("Background task complete"),
        Duration::from_secs(3),
    )?;

    host.wait_for_text("[image: sample.png unavailable]", Duration::from_secs(2))?;
    host.wait_for_text("line-00000", Duration::from_secs(2))?;
    host.wait_for_text("press e to expand all", Duration::from_secs(2))?;
    assert!(
        !host.screen_contents()?.contains("line-09999"),
        "last line should not render before expanding the large block"
    );

    let expected_link = osc8_hyperlink_sequence("https://example.test/docs", "Open docs");
    host.wait_for_output(expected_link.as_bytes(), Duration::from_secs(2))?;

    host.send_str("e")?;
    host.key_with_mods(KeyCode::End, KeyModifiers::NONE)?;
    host.wait_for_text("line-09999", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
