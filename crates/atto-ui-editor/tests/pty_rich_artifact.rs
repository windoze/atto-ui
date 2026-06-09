use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use atto_ui_test_host::PtyTestHost;
use unicode_width::UnicodeWidthStr;

static RICH_ARTIFACT_PTY_LOCK: Mutex<()> = Mutex::new(());

fn rich_artifact_pty_lock() -> MutexGuard<'static, ()> {
    RICH_ARTIFACT_PTY_LOCK
        .lock()
        .expect("rich artifact PTY lock poisoned")
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

#[test]
fn rich_artifact_code_link_opens_syntax_highlighted_editor_view() -> anyhow::Result<()> {
    let _guard = rich_artifact_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_rich_artifact_app");
    let mut host = PtyTestHost::spawn(bin, &[], 100, 28)?;

    host.wait_for_text("Artifact Code: main.rs", Duration::from_secs(3))?;
    let (x, y) = find_text_position(&host, "Artifact Code: main.rs").expect("code link");
    host.click(x, y)?;

    host.wait_for_text("Code: main.rs", Duration::from_secs(3))?;
    host.wait_for_text("CODE-ARTIFACT", Duration::from_secs(3))?;

    let (fn_x, fn_y) = find_text_position(&host, "fn main").expect("fn main in code viewer");
    let fn_color = host.cell_fgcolor(fn_x, fn_y)?;
    let main_color = host.cell_fgcolor(fn_x + 3, fn_y)?;
    let screen = host.screen_contents()?;
    assert_ne!(
        fn_color, main_color,
        "rich code viewer should apply syntax colors at ({fn_x}, {fn_y})\n--- screen ---\n{screen}"
    );

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn rich_artifact_diff_link_uses_diff_view_with_hunk_folding() -> anyhow::Result<()> {
    let _guard = rich_artifact_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_rich_artifact_app");
    let mut host = PtyTestHost::spawn(bin, &[], 100, 28)?;

    host.wait_for_text("Artifact Diff: main.patch", Duration::from_secs(3))?;
    let (x, y) = find_text_position(&host, "Artifact Diff: main.patch").expect("diff link");
    host.click(x, y)?;

    host.wait_for_text("Diff: main.patch", Duration::from_secs(3))?;
    host.wait_for_text("DIFF-ARTIFACT", Duration::from_secs(3))?;

    host.send_str("z")?;
    host.wait_for_screen(
        |snapshot| {
            let joined = snapshot.join("\n");
            joined.contains("[+] hunk 1 collapsed") && !joined.contains("DIFF-ARTIFACT")
        },
        Duration::from_secs(3),
    )?;

    host.send_str("z")?;
    host.wait_for_text("DIFF-ARTIFACT", Duration::from_secs(3))?;

    host.send_ctrl('q')?;
    Ok(())
}
