#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};

const PTY_WAIT: Duration = Duration::from_secs(5);

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("atto_editor_app_{prefix}_{nanos}"))
}

fn wait_for_file_contains(path: &Path, needle: &str, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if fs::read_to_string(path)
            .unwrap_or_default()
            .contains(needle)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    anyhow::bail!(
        "timed out waiting for {:?} to contain {:?}; content was {:?}",
        path,
        needle,
        fs::read_to_string(path).unwrap_or_default()
    )
}

#[test]
fn command_palette_ctrl_shift_p_can_run_save() -> anyhow::Result<()> {
    let root = unique_temp_dir("command_palette_save");
    fs::create_dir_all(&root)?;
    let file = root.join("main.rs");
    fs::write(&file, "fn main() {}\n")?;

    let exe = PathBuf::from(env!("CARGO_BIN_EXE_atto-editor-app"));
    let mut host = PtyTestHost::spawn(&exe, &[file.to_string_lossy().as_ref()], 100, 28)?;

    host.wait_for_text("fn main", PTY_WAIT)?;
    host.send_str("// saved via palette\n")?;
    host.wait_for_text("// saved via palette", PTY_WAIT)?;

    host.key_with_mods(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )?;
    host.wait_for_text("Command Palette", PTY_WAIT)?;
    host.send_str("save")?;
    host.wait_for_text("Save", PTY_WAIT)?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_screen(
        |rows| !rows.join("\n").contains("Command Palette"),
        PTY_WAIT,
    )?;

    wait_for_file_contains(&file, "// saved via palette", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
