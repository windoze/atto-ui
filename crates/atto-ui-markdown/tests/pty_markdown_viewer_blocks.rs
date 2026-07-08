use std::time::Duration;

use atto_ui_test_host::PtyTestHost;
use unicode_width::UnicodeWidthStr;

const WAIT: Duration = Duration::from_secs(5);

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

    let (rust_keyword_x, rust_keyword_y) =
        find_text_position(&host, "fn syntax_demo").expect("rust keyword position");
    let (python_keyword_x, python_keyword_y) =
        find_text_position(&host, "def syntax_demo").expect("python keyword position");
    let (fallback_x, fallback_y) =
        find_text_position(&host, "UNKNOWN-HIGHLIGHT").expect("fallback code position");

    let fallback_fg = host.cell_fgcolor(fallback_x, fallback_y)?;
    assert_ne!(
        host.cell_fgcolor(rust_keyword_x, rust_keyword_y)?,
        fallback_fg,
        "rust keyword should use syntax highlighting instead of fallback code color"
    );
    assert_ne!(
        host.cell_fgcolor(python_keyword_x, python_keyword_y)?,
        fallback_fg,
        "python keyword should use syntax highlighting instead of fallback code color"
    );

    host.send_ctrl('q')?;
    host.wait_for_exit(Duration::from_secs(2))?;
    Ok(())
}
