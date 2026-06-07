use std::time::Duration;

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};

const WAIT: Duration = Duration::from_secs(5);

fn find_text_pos(screen: &str, needle: &str) -> Option<(usize, usize)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(col) = line.find(needle) {
            return Some((row, col));
        }
    }
    None
}

fn send_left_drag_without_release(
    host: &mut PtyTestHost,
    x0: u16,
    y0: u16,
    x1: u16,
    y1: u16,
) -> anyhow::Result<()> {
    let x0 = x0.saturating_add(1);
    let y0 = y0.saturating_add(1);
    let x1 = x1.saturating_add(1);
    let y1 = y1.saturating_add(1);
    host.send_str(&format!("\x1b[<0;{x0};{y0}M"))?;
    host.send_str(&format!("\x1b[<32;{x1};{y1}M"))?;
    Ok(())
}

fn send_left_release(host: &mut PtyTestHost, x: u16, y: u16) -> anyhow::Result<()> {
    let x = x.saturating_add(1);
    let y = y.saturating_add(1);
    host.send_str(&format!("\x1b[<0;{x};{y}m"))
}

#[test]
fn pty_component_drag_drop_updates_target() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--drag-drop"], 80, 24)?;

    host.wait_for_text("Drag source", WAIT)?;
    host.wait_for_text("Drop target", WAIT)?;

    let screen = host.screen_contents()?;
    let (source_row, source_col) = find_text_pos(&screen, "drag-item").expect("source item");
    let (target_row, target_col) = find_text_pos(&screen, "Drop target").expect("target");

    host.drag_left(
        source_col as u16,
        source_row as u16,
        target_col as u16,
        target_row as u16,
    )?;
    host.wait_for_text("Dropped: drag-item", WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn pty_component_drag_esc_cancels_without_drop() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &["--drag-drop"], 80, 24)?;

    host.wait_for_text("Drag source", WAIT)?;
    host.wait_for_text("Drop target", WAIT)?;

    let screen = host.screen_contents()?;
    let (source_row, source_col) = find_text_pos(&screen, "drag-item").expect("source item");
    let (target_row, target_col) = find_text_pos(&screen, "Drop target").expect("target");

    send_left_drag_without_release(
        &mut host,
        source_col as u16,
        source_row as u16,
        target_col as u16,
        target_row as u16,
    )?;
    host.key_with_mods(KeyCode::Esc, KeyModifiers::NONE)?;
    send_left_release(&mut host, target_col as u16, target_row as u16)?;

    host.wait_for_text("Cancelled: yes", WAIT)?;
    let screen = host.screen_contents()?;
    assert!(
        !screen.contains("Dropped:"),
        "Esc-cancelled drag should not drop\n--- screen ---\n{screen}"
    );

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
