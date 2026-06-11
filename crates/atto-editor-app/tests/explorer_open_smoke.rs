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
fn double_click_in_explorer_opens_file_without_hanging() -> anyhow::Result<()> {
    let root = unique_temp_dir("explorer_open_smoke");
    fs::create_dir_all(&root)?;

    let file_name = "open_me.txt";
    let file_path = root.join(file_name);
    fs::write(&file_path, "HELLO_FROM_EDITOR\n")?;

    let exe = PathBuf::from(env!("CARGO_BIN_EXE_atto-editor-app"));
    let mut host = PtyTestHost::spawn(&exe, &[root.to_string_lossy().as_ref()], 90, 28)?;

    // Wait for the Explorer window to render the file name.
    host.wait_for_text(file_name, PTY_WAIT)?;

    // Default layout places the Explorer window docked left. The file tree is
    // borderless, so rows are: menu (0), window border (1), root dir (2), first
    // child file (3).
    let click_x = 6;
    let click_y = 3;
    host.click(click_x, click_y)?;
    std::thread::sleep(Duration::from_millis(60));
    host.click(click_x, click_y)?;

    // The editor view should render the file contents somewhere on screen.
    host.wait_for_text("HELLO_FROM_EDITOR", PTY_WAIT)?;

    // Ensure the app is still responsive (Quit shortcut should work).
    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
