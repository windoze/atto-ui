#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};

const PTY_WAIT: Duration = Duration::from_secs(5);

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("atto_editor_app_{prefix}_{nanos}"))
}

#[test]
fn file_picker_ctrl_p_can_open_workspace_file() -> anyhow::Result<()> {
    let root = unique_temp_dir("file_picker_open");
    fs::create_dir_all(root.join("src"))?;
    let file = root.join("src").join("main.rs");
    fs::write(&file, "fn main() { println!(\"picker\"); }\n")?;

    let exe = PathBuf::from(env!("CARGO_BIN_EXE_atto-editor-app"));
    let mut host = PtyTestHost::spawn(&exe, &[root.to_string_lossy().as_ref()], 100, 28)?;

    host.wait_for_text("Atto Editor", PTY_WAIT)?;
    host.key_with_mods(KeyCode::Char('p'), KeyModifiers::CONTROL)?;
    host.wait_for_text("File Picker", PTY_WAIT)?;
    host.send_str("src/main")?;
    host.wait_for_text("src/main.rs", PTY_WAIT)?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("println", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
