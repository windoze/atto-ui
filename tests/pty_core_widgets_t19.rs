use std::time::Duration;

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn pty_core_widgets_cover_t19_state_and_hit_paths() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--core-widgets-t19"], 100, 32)?;

    host.wait_for_text("T19 status button=0 radio=0 list=0 table=0 slider=0", WAIT)?;
    host.wait_for_text("Fire", WAIT)?;
    host.wait_for_text("Disabled", WAIT)?;
    host.wait_for_text("Progress", WAIT)?;
    host.wait_for_text("Loading", WAIT)?;
    host.wait_for_text("Grid-00", WAIT)?;

    host.click(4, 6)?;
    host.wait_for_text("T19 status button=1 radio=0 list=0 table=0 slider=0", WAIT)?;

    host.click(4, 12)?;
    host.wait_for_text("T19 status button=1 radio=2 list=0 table=0 slider=0", WAIT)?;
    host.key_with_mods(KeyCode::Up, KeyModifiers::NONE)?;
    host.wait_for_text("T19 status button=1 radio=1 list=0 table=0 slider=0", WAIT)?;

    host.click(36, 9)?;
    host.wait_for_text("T19 status button=1 radio=1 list=0 table=0 slider=5", WAIT)?;
    host.key_with_mods(KeyCode::Right, KeyModifiers::NONE)?;
    host.wait_for_text("T19 status button=1 radio=1 list=0 table=0 slider=6", WAIT)?;

    host.click(4, 18)?;
    host.wait_for_text("T19 status button=1 radio=1 list=3 table=0 slider=6", WAIT)?;

    host.click(30, 18)?;
    host.wait_for_text("T19 status button=1 radio=1 list=3 table=2 slider=6", WAIT)?;

    host.wheel_down(4, 24)?;
    host.wait_for_text("Grid-10", WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
