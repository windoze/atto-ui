use std::time::Duration;

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};
use unicode_width::UnicodeWidthStr;

const WAIT: Duration = Duration::from_secs(5);

fn find_text_pos(screen: &str, needle: &str) -> Option<(u16, u16)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte_idx) = line.find(needle) {
            let col = UnicodeWidthStr::width(&line[..byte_idx]);
            return Some((col as u16, row as u16));
        }
    }
    None
}

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

    let screen = host.screen_contents()?;
    let (fire_x, fire_y) = find_text_pos(&screen, "Fire")
        .ok_or_else(|| anyhow::anyhow!("Fire button not found\n--- screen ---\n{screen}"))?;
    assert_ne!(
        host.cell_contents(fire_x, fire_y.saturating_sub(1))?,
        "─",
        "button should no longer render a top border\n--- screen ---\n{screen}"
    );
    assert_ne!(
        host.cell_contents(fire_x.saturating_sub(1), fire_y)?,
        "│",
        "button should no longer render a side border\n--- screen ---\n{screen}"
    );

    host.click(4, 5)?;
    host.wait_for_text("T19 status button=1 radio=0 list=0 table=0 slider=0", WAIT)?;

    host.click(4, 10)?;
    host.wait_for_text("T19 status button=1 radio=2 list=0 table=0 slider=0", WAIT)?;
    host.key_with_mods(KeyCode::Up, KeyModifiers::NONE)?;
    host.wait_for_text("T19 status button=1 radio=1 list=0 table=0 slider=0", WAIT)?;

    host.click(36, 7)?;
    host.wait_for_text("T19 status button=1 radio=1 list=0 table=0 slider=5", WAIT)?;
    host.key_with_mods(KeyCode::Right, KeyModifiers::NONE)?;
    host.wait_for_text("T19 status button=1 radio=1 list=0 table=0 slider=6", WAIT)?;

    host.click(4, 16)?;
    host.wait_for_text("T19 status button=1 radio=1 list=3 table=0 slider=6", WAIT)?;

    host.click(30, 16)?;
    host.wait_for_text("T19 status button=1 radio=1 list=3 table=2 slider=6", WAIT)?;

    host.wheel_down(4, 22)?;
    host.wait_for_text("Grid-10", WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
