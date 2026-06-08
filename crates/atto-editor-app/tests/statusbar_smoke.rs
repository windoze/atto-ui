#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use atto_ui_test_host::PtyTestHost;

const PTY_WAIT: Duration = Duration::from_secs(5);

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("atto_editor_app_{prefix}_{nanos}"))
}

#[test]
fn statusbar_shows_editor_diagnostics_and_language() -> anyhow::Result<()> {
    let root = unique_temp_dir("statusbar_smoke");
    fs::create_dir_all(&root)?;
    let file = root.join("main.rs");
    fs::write(&file, "fn main() { println!(\"hello\"); }\n")?;

    let exe = PathBuf::from(env!("CARGO_BIN_EXE_atto-editor-app"));
    let mut host = PtyTestHost::spawn(&exe, &[file.to_string_lossy().as_ref()], 100, 28)?;

    host.wait_for_text("fn main", PTY_WAIT)?;
    host.wait_for_text("E:0 W:0", PTY_WAIT)?;
    host.wait_for_text("rust", PTY_WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
