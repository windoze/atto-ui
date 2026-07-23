use std::time::Duration;

use atto_ui_test_host::PtyTestHost;

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn pty_markdown_viewer_renders_heading_list_quote_code_and_table() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_markdown_app");
    let mut host = PtyTestHost::spawn(bin, &["--blocks"], 80, 28)?;

    host.wait_for_text("T19 Heading", WAIT)?;
    host.wait_for_text("parent item", WAIT)?;
    host.wait_for_text("nested child item", WAIT)?;
    host.wait_for_text("quoted line", WAIT)?;
    host.wait_for_text("println!", WAIT)?;
    host.wait_for_text("Feature", WAIT)?;
    host.wait_for_text("heading", WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}

#[test]
fn pty_markdown_viewer_renders_syntax_highlighted_code_blocks() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_snapshot_markdown_app");
    let mut host = PtyTestHost::spawn(bin, &["--syntax-highlighting"], 90, 30)?;

    host.wait_for_text("fn syntax_demo", WAIT)?;
    host.wait_for_text("RUST-HIGHLIGHT", WAIT)?;
    host.wait_for_text("def syntax_demo", WAIT)?;
    host.wait_for_text("PY-HIGHLIGHT", WAIT)?;
    host.wait_for_text("UNKNOWN-HIGHLIGHT", WAIT)?;

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
