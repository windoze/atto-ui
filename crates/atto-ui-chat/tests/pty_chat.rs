use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use atto_ui::clipboard::osc52_sequence;
use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};
use unicode_width::UnicodeWidthStr;

static CHAT_PTY_LOCK: Mutex<()> = Mutex::new(());

fn chat_pty_lock() -> MutexGuard<'static, ()> {
    CHAT_PTY_LOCK.lock().expect("chat PTY lock poisoned")
}

fn assert_text_absent_for(host: &PtyTestHost, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let screen = host.screen_contents().unwrap_or_default();
        if screen.contains(needle) {
            panic!("expected text {needle:?} to remain absent.\n--- screen ---\n{screen}");
        }
        thread::sleep(Duration::from_millis(10));
    }
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

fn find_last_text_position(host: &PtyTestHost, needle: &str) -> Option<(u16, u16)> {
    let contents = host.screen_contents().ok()?;
    let mut found = None;
    for (y, line) in contents.lines().enumerate() {
        for (byte_idx, _) in line.match_indices(needle) {
            let x = UnicodeWidthStr::width(&line[..byte_idx]);
            found = Some((
                x.min(u16::MAX as usize) as u16,
                y.min(u16::MAX as usize) as u16,
            ));
        }
    }
    found
}

fn find_close_button_on_title_row(host: &PtyTestHost, title: &str) -> Option<(u16, u16)> {
    let contents = host.screen_contents().ok()?;
    contents.lines().enumerate().find_map(|(y, line)| {
        line.find(title)?;
        let close_idx = line.find("[■]")?;
        let glyph_idx = close_idx.saturating_add("[".len());
        Some((
            line[..glyph_idx].width().min(u16::MAX as usize) as u16,
            y.min(u16::MAX as usize) as u16,
        ))
    })
}

#[test]
fn chat_input_modes_switch() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("Message", Duration::from_secs(2))?;

    host.send_str("c")?;
    host.wait_for_text("请选择一种回应方式", Duration::from_secs(2))?;
    host.wait_for_text("简短回复", Duration::from_secs(2))?;

    host.send_str("f")?;
    host.wait_for_text("是否继续执行?", Duration::from_secs(2))?;
    host.wait_for_text("继续", Duration::from_secs(2))?;

    host.send_str("t")?;
    host.wait_for_text("Message", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_input_modes_submit_callbacks() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("Message", Duration::from_secs(2))?;

    host.send_str("hello")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=hello", Duration::from_secs(2))?;

    host.send_str("c")?;
    host.wait_for_text("请选择一种回应方式", Duration::from_secs(2))?;
    host.key_with_mods(KeyCode::Down, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Tab, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: choice index=1", Duration::from_secs(2))?;
    host.wait_for_text("label=详细解释", Duration::from_secs(2))?;

    host.send_str("f")?;
    host.wait_for_text("是否继续执行?", Duration::from_secs(2))?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: choice index=0", Duration::from_secs(2))?;
    host.wait_for_text("label=继续", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_slash_completion_filters_selects_accepts_and_closes() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--input-completion"], 100, 30)?;

    host.wait_for_text("COMPLETION-READY", Duration::from_secs(2))?;

    host.send_str("/")?;
    host.wait_for_text("COMMAND-MODEL", Duration::from_secs(2))?;
    host.wait_for_text("COMMAND-MERGE", Duration::from_secs(2))?;
    host.wait_for_text("COMMAND-CLEAR", Duration::from_secs(2))?;
    host.send_str("m")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot.iter().any(|line| line.contains("/m"))
                && snapshot.iter().all(|line| !line.contains("COMMAND-CLEAR"))
        },
        Duration::from_secs(2),
    )?;

    host.key_with_mods(KeyCode::Down, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_screen(
        |snapshot| {
            snapshot.iter().any(|line| line.contains("/merge ready"))
                && snapshot.iter().all(|line| !line.contains("COMMAND-MERGE"))
        },
        Duration::from_secs(2),
    )?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=/merge ready", Duration::from_secs(2))?;

    host.send_str("/")?;
    host.send_str("cl")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot.iter().any(|line| line.contains("COMMAND-CLEAR"))
                && snapshot.iter().all(|line| !line.contains("COMMAND-MODEL"))
                && snapshot.iter().all(|line| !line.contains("COMMAND-MERGE"))
        },
        Duration::from_secs(2),
    )?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text(
        "SLASH_COMMAND: id=clear label=/clear",
        Duration::from_secs(2),
    )?;

    host.send_str("/")?;
    host.wait_for_text("COMMAND-MODEL", Duration::from_secs(2))?;
    host.key_with_mods(KeyCode::Esc, KeyModifiers::NONE)?;
    host.wait_for_screen(
        |snapshot| snapshot.iter().all(|line| !line.contains("COMMAND-MODEL")),
        Duration::from_secs(2),
    )?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_mention_completion_inserts_file_and_closes() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--input-completion"], 100, 30)?;

    host.wait_for_text("COMPLETION-READY", Duration::from_secs(2))?;

    host.send_str("open @")?;
    host.wait_for_text("FILE-CARGO", Duration::from_secs(2))?;
    host.send_str("ca")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot.iter().any(|line| line.contains("open @ca"))
                && snapshot.iter().any(|line| line.contains("FILE-CARGO"))
                && snapshot.iter().all(|line| !line.contains("FILE-LIB"))
        },
        Duration::from_secs(2),
    )?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("open @Cargo.toml"))
                && snapshot.iter().all(|line| !line.contains("FILE-CARGO"))
        },
        Duration::from_secs(2),
    )?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=open @Cargo.toml", Duration::from_secs(2))?;

    host.send_str("inspect @")?;
    host.wait_for_text("FILE-LIB", Duration::from_secs(2))?;
    host.send_str("sr")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot.iter().any(|line| line.contains("inspect @sr"))
                && snapshot.iter().any(|line| line.contains("FILE-LIB"))
                && snapshot.iter().any(|line| line.contains("FILE-MAIN"))
        },
        Duration::from_secs(2),
    )?;
    host.key_with_mods(KeyCode::Esc, KeyModifiers::NONE)?;
    host.wait_for_screen(
        |snapshot| snapshot.iter().all(|line| !line.contains("FILE-LIB")),
        Duration::from_secs(2),
    )?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_textarea_multiline_history_and_kill_ring() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 26)?;

    host.wait_for_text("Message", Duration::from_secs(2))?;

    host.send_str("line1")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::SHIFT)?;
    host.send_str("line2")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot.iter().any(|line| line.contains("line1"))
                && snapshot.iter().any(|line| line.contains("line2"))
        },
        Duration::from_secs(2),
    )?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=line1", Duration::from_secs(2))?;
    host.wait_for_text("line2", Duration::from_secs(2))?;

    host.send_str("uno")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=uno", Duration::from_secs(2))?;
    host.send_str("zwo")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=zwo", Duration::from_secs(2))?;

    host.key_with_mods(KeyCode::Up, KeyModifiers::NONE)?;
    host.send_str("-zz")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=zwo-zz", Duration::from_secs(2))?;

    host.key_with_mods(KeyCode::Up, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Up, KeyModifiers::NONE)?;
    host.key_with_mods(KeyCode::Down, KeyModifiers::NONE)?;
    host.send_str("-yy")?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=zwo-zz-yy", Duration::from_secs(2))?;

    host.send_str("killme")?;
    host.send_ctrl('u')?;
    host.send_ctrl('y')?;
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)?;
    host.wait_for_text("SUBMIT: text=killme", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_auto_follow_pauses_after_user_scrolls_up() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("MSG-27", Duration::from_secs(2))?;

    host.send_str("a")?;
    host.wait_for_text("FOLLOW-1", Duration::from_secs(2))?;

    for _ in 0..24 {
        host.wheel_up(6, 6)?;
        thread::sleep(Duration::from_millis(30));
        if host.screen_contents()?.contains("MSG-15") {
            break;
        }
    }
    host.wait_for_text("MSG-15", Duration::from_secs(2))?;

    host.send_str("b")?;
    assert_text_absent_for(&host, "FOLLOW-2", Duration::from_millis(250));

    for _ in 0..30 {
        host.wheel_down(6, 6)?;
        thread::sleep(Duration::from_millis(30));
    }
    host.wait_for_text("FOLLOW-2", Duration::from_secs(2))?;

    host.send_str("d")?;
    host.wait_for_text("FOLLOW-3", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_load_more_on_scroll_top() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("MSG-27", Duration::from_secs(2))?;

    for _ in 0..80 {
        host.wheel_up(6, 6)?;
        thread::sleep(Duration::from_millis(60));
        if host.screen_contents()?.contains("MSG-00") {
            break;
        }
    }

    host.wait_for_text("MSG-00", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "HISTORY-1-", Duration::from_millis(250));

    for _ in 0..2 {
        host.wheel_up(6, 6)?;
        thread::sleep(Duration::from_millis(60));
    }
    host.wait_for_text("HISTORY-1-2", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_streaming_markdown_tolerates_incomplete_blocks() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--streaming-markdown"], 80, 24)?;

    host.wait_for_text("STREAMING-MARKDOWN", Duration::from_secs(2))?;

    host.send_str("1")?;
    host.wait_for_text("STREAMING-CODE", Duration::from_secs(2))?;
    host.wait_for_text("fn main()", Duration::from_secs(2))?;

    host.send_str("2")?;
    host.wait_for_text("STREAMING-CODE", Duration::from_secs(2))?;

    host.send_str("3")?;
    host.wait_for_text("| half |", Duration::from_secs(2))?;
    let partial = host.screen_snapshot()?;
    assert!(partial.iter().any(|line| line.contains("| Name | Value |")));
    assert!(partial.iter().all(|line| !line.contains('┬')));

    host.send_str("4")?;
    host.wait_for_text("stable", Duration::from_secs(2))?;
    let complete = host.screen_snapshot()?;
    assert!(complete.iter().any(|line| line.contains('┬')));

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_streaming_delta_append_renders_accumulated_text() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--streaming-markdown"], 80, 24)?;

    host.wait_for_text("STREAMING-MARKDOWN", Duration::from_secs(2))?;

    host.send_str("5")?;
    host.wait_for_text("STREAM-DELTA-A", Duration::from_secs(2))?;

    host.send_str("6")?;
    host.wait_for_text("STREAM-DELTA-A + STREAM-DELTA-B", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_tool_call_disclosure_streams_status_and_toggles() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--tool-call"], 80, 24)?;

    host.wait_for_text("build", Duration::from_secs(2))?;
    host.wait_for_text("Input: cargo build --workspace", Duration::from_secs(2))?;
    host.wait_for_text("Tool result: tool-1", Duration::from_secs(2))?;
    host.wait_for_text("[~]", Duration::from_secs(2))?;
    host.wait_for_text("TOOL-START", Duration::from_secs(2))?;

    host.send_str("1")?;
    host.wait_for_text("TOOL-OUTPUT-1", Duration::from_secs(2))?;

    host.send_str("2")?;
    host.wait_for_text("TOOL-OUTPUT-2", Duration::from_secs(2))?;

    host.send_str("3")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("[x]") && line.contains("build"))
        },
        Duration::from_secs(2),
    )?;

    let (x, y) = find_text_position(&host, "Tool result: tool-1").expect("tool result position");
    host.click(x, y)?;
    host.wait_for_screen(
        |snapshot| snapshot.iter().all(|line| !line.contains("TOOL-OUTPUT-1")),
        Duration::from_secs(2),
    )?;

    host.click(x, y)?;
    host.wait_for_text("TOOL-OUTPUT-2", Duration::from_secs(2))?;

    host.send_str("4")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("[!]") && line.contains("build"))
        },
        Duration::from_secs(2),
    )?;

    host.send_str("5")?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("[-]") && line.contains("build"))
        },
        Duration::from_secs(2),
    )?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_inline_approval_buttons_emit_and_lock() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--inline-approval"], 100, 28)?;

    host.wait_for_text("inline_approval", Duration::from_secs(2))?;
    host.wait_for_text("Input: INLINE-APPROVAL-COMMAND", Duration::from_secs(2))?;
    host.wait_for_text(
        "Approval: Run INLINE-APPROVAL-COMMAND?",
        Duration::from_secs(2),
    )?;
    host.wait_for_text("Allow once", Duration::from_secs(2))?;
    host.wait_for_text("Allow always", Duration::from_secs(2))?;
    host.wait_for_text("Deny", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "Allow always").expect("allow always button");
    host.click(x, y)?;

    host.wait_for_text(
        "APPROVED: approval-inline/allow_always",
        Duration::from_secs(2),
    )?;
    host.wait_for_text("[x] Allow always", Duration::from_secs(2))?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("[~]") && line.contains("inline_approval"))
        },
        Duration::from_secs(2),
    )?;

    assert_text_absent_for(&host, "Deny", Duration::from_millis(250));
    assert_text_absent_for(&host, "option=deny", Duration::from_millis(250));

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_inline_diff_buttons_emit_and_lock() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--inline-diff"], 100, 28)?;

    host.wait_for_text("Diff: src/inline_diff.rs (pending)", Duration::from_secs(2))?;
    host.wait_for_text("+NEW-INLINE-DIFF", Duration::from_secs(2))?;
    host.wait_for_text("Accept", Duration::from_secs(2))?;
    host.wait_for_text("Reject", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "Accept").expect("accept button");
    host.click(x, y)?;

    host.wait_for_text("EDIT_DECISION: 1001/accepted", Duration::from_secs(2))?;
    host.wait_for_text(
        "Diff: src/inline_diff.rs (accepted)",
        Duration::from_secs(2),
    )?;
    host.wait_for_text("[x] Accepted", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "Reject", Duration::from_millis(250));

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_syntax_diff_highlights_context_and_preserves_semantic_lines() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--syntax-diff"], 100, 28)?;

    host.wait_for_text("Diff: src/syntax_diff.rs (pending)", Duration::from_secs(2))?;
    host.wait_for_text("fn syntax_diff", Duration::from_secs(2))?;
    host.wait_for_text("let stable_value = 0;", Duration::from_secs(2))?;
    host.wait_for_text("-    let old_value = 1;", Duration::from_secs(2))?;
    host.wait_for_text("+    let new_value = 42;", Duration::from_secs(2))?;
    host.wait_for_text("+    println!(\"DIFF-SYNTAX\");", Duration::from_secs(2))?;

    let (context_keyword_x, context_keyword_y) =
        find_text_position(&host, "let stable_value").expect("context keyword position");
    let (context_value_x, context_value_y) =
        find_text_position(&host, "stable_value").expect("context variable position");
    assert_eq!(context_keyword_y, context_value_y);
    assert_ne!(
        host.cell_fgcolor(context_keyword_x, context_keyword_y)?,
        host.cell_fgcolor(context_value_x, context_value_y)?,
        "context diff payload should receive syntax-level coloring"
    );

    let (add_prefix_x, add_y) =
        find_text_position(&host, "+    let new_value").expect("addition prefix position");
    let (add_keyword_x, add_keyword_y) =
        find_text_position(&host, "let new_value").expect("addition keyword position");
    assert_eq!(add_y, add_keyword_y);
    let addition_fg = host.cell_fgcolor(add_prefix_x, add_y)?;
    assert_eq!(
        host.cell_fgcolor(add_keyword_x, add_keyword_y)?,
        addition_fg,
        "addition syntax spans should preserve the semantic foreground"
    );
    assert_eq!(
        host.cell_bgcolor(add_keyword_x, add_keyword_y)?,
        host.cell_bgcolor(add_prefix_x, add_y)?,
        "addition syntax spans should preserve the semantic background"
    );

    let (remove_prefix_x, remove_y) =
        find_text_position(&host, "-    let old_value").expect("removal prefix position");
    let (remove_keyword_x, remove_keyword_y) =
        find_text_position(&host, "let old_value").expect("removal keyword position");
    assert_eq!(remove_y, remove_keyword_y);
    let removal_fg = host.cell_fgcolor(remove_prefix_x, remove_y)?;
    assert_eq!(
        host.cell_fgcolor(remove_keyword_x, remove_keyword_y)?,
        removal_fg,
        "removal syntax spans should preserve the semantic foreground"
    );
    assert_ne!(
        addition_fg, removal_fg,
        "addition and removal lines should keep distinct semantic colors"
    );

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_plan_mode_buttons_emit_and_lock() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--plan-mode"], 100, 28)?;

    host.wait_for_text("Plan: pending", Duration::from_secs(2))?;
    host.wait_for_text("PLAN-STEP-1", Duration::from_secs(2))?;
    host.wait_for_text("PLAN-STEP-2", Duration::from_secs(2))?;
    host.wait_for_text("Accept", Duration::from_secs(2))?;
    host.wait_for_text("Reject", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "Accept").expect("accept button");
    host.click(x, y)?;

    host.wait_for_text("PLAN_DECISION: 1001/accepted", Duration::from_secs(2))?;
    host.wait_for_text("Plan: accepted", Duration::from_secs(2))?;
    host.wait_for_text("[x] Accepted", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "Reject", Duration::from_millis(250));

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_nested_task_block_renders_updates_and_virtualizes() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--nested-task"], 100, 28)?;

    host.wait_for_text("TASK-TRAIL-04", Duration::from_secs(2))?;
    for _ in 0..24 {
        host.wheel_up(8, 10)?;
        thread::sleep(Duration::from_millis(25));
        if host.screen_contents()?.contains("Task: SUBAGENT-SEARCH") {
            break;
        }
    }
    host.wait_for_text("Task: SUBAGENT-SEARCH", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "NESTED-SEARCH", Duration::from_millis(160));

    let (x, y) = find_text_position(&host, "Task: SUBAGENT-SEARCH").expect("nested task title");
    host.click(x, y)?;
    host.wait_for_text("Status: running", Duration::from_secs(2))?;
    host.wait_for_text("SUBAGENT-INITIAL", Duration::from_secs(2))?;
    host.wait_for_text("NESTED-SEARCH", Duration::from_secs(2))?;
    host.wait_for_text("Tool use: grep", Duration::from_secs(2))?;

    host.send_str("1")?;
    host.wait_for_text("Status: complete", Duration::from_secs(2))?;
    host.wait_for_text("SUBAGENT-DONE", Duration::from_secs(2))?;
    host.wait_for_text("NESTED-FINAL", Duration::from_secs(2))?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("[x]") && line.contains("Task: SUBAGENT-SEARCH"))
        },
        Duration::from_secs(2),
    )?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_tool_result_long_ansi_output_tails_streams_and_expands() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--long-tool-output"], 100, 36)?;

    host.wait_for_text("long_tool", Duration::from_secs(2))?;
    host.wait_for_text("Input: generate long output", Duration::from_secs(2))?;
    host.wait_for_text("展开全部", Duration::from_secs(2))?;
    host.wait_for_text("TOOL-LONG-LINE-29", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "TOOL-LONG-LINE-00", Duration::from_millis(120));

    host.send_str("1")?;
    host.wait_for_text("TOOL-LONG-STREAMED", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "TOOL-LONG-LINE-00", Duration::from_millis(120));

    let (x, y) = find_text_position(&host, "展开全部").expect("expand all action");
    host.click(x, y)?;
    host.wait_for_text("TOOL-LONG-LINE-00", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_thinking_notice_renders_collapsed_and_level_labels() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--thinking-notice"], 100, 28)?;

    host.wait_for_text("Thinking", Duration::from_secs(2))?;
    host.wait_for_text("Info: CONTEXT-INFO", Duration::from_secs(2))?;
    host.wait_for_text("Warning: CONTEXT-WARNING", Duration::from_secs(2))?;
    host.wait_for_text("Error: CONTEXT-ERROR", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "THINKING-DETAIL", Duration::from_millis(250));

    let (x, y) = find_text_position(&host, "Thinking").expect("thinking disclosure");
    host.click(x, y)?;
    host.wait_for_text("THINKING-DETAIL", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_todo_panel_renders_and_updates_state() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--todo-panel"], 100, 28)?;

    host.wait_for_text("[ ] TODO-PLAN", Duration::from_secs(2))?;
    host.wait_for_text("[~] TODO-IMPLEMENT", Duration::from_secs(2))?;
    host.wait_for_text("[ ] TODO-VERIFY", Duration::from_secs(2))?;

    host.send_str("1")?;
    host.wait_for_text("[x] TODO-PLAN", Duration::from_secs(2))?;
    host.wait_for_text("[x] TODO-IMPLEMENT", Duration::from_secs(2))?;
    host.wait_for_text("[~] TODO-VERIFY", Duration::from_secs(2))?;
    assert_text_absent_for(&host, "[~] TODO-IMPLEMENT", Duration::from_millis(120));

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_turn_header_renders_meta_and_structured_error() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--turn-meta-error"], 110, 34)?;

    host.wait_for_text("model: claude-sonnet-test", Duration::from_secs(2))?;
    host.wait_for_text("usage: 1234 input/56 output", Duration::from_secs(2))?;
    host.wait_for_text("elapsed: 1530ms", Duration::from_secs(2))?;
    host.wait_for_text("stop: tool_use", Duration::from_secs(2))?;
    host.wait_for_text("failed", Duration::from_secs(2))?;
    host.wait_for_text("Error kind: network", Duration::from_secs(2))?;
    host.wait_for_text("Error message: TURN-ERROR-MESSAGE", Duration::from_secs(2))?;
    host.wait_for_text("Error detail: TURN-ERROR-DETAIL", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_message_action_buttons_emit_turn_and_block_actions() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--message-actions"], 110, 32)?;

    host.wait_for_text("ACTION-USER-MESSAGE", Duration::from_secs(2))?;
    host.wait_for_text("ACTION-ASSISTANT-MESSAGE", Duration::from_secs(2))?;
    host.wait_for_text("ACTION-ASSISTANT-RETRY-MESSAGE", Duration::from_secs(2))?;
    host.wait_for_text("Retry", Duration::from_secs(2))?;
    host.wait_for_text("Regenerate", Duration::from_secs(2))?;
    host.wait_for_text("Copy block", Duration::from_secs(2))?;

    for _ in 0..6 {
        host.wheel_up(5, 6)?;
    }
    host.wait_for_text("Edit", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "ACTION-USER-MESSAGE").expect("copy target body");
    host.click(x, y)?;
    host.key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL)?;
    host.wait_for_text("MESSAGE_ACTION: 1/copy_block:1001", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "Copy").expect("copy action");
    host.click(x, y)?;
    host.wait_for_screen(
        |snapshot| {
            snapshot
                .iter()
                .any(|line| line.contains("MESSAGE_ACTION: 1/copy") && !line.contains("copy_block"))
        },
        Duration::from_secs(2),
    )?;

    let (x, y) = find_text_position(&host, "Edit").expect("edit user action");
    host.click(x, y)?;
    host.wait_for_text("MESSAGE_ACTION: 1/edit_user", Duration::from_secs(2))?;

    for _ in 0..6 {
        host.wheel_down(5, 20)?;
    }
    host.wait_for_text("ACTION-ASSISTANT-RETRY-MESSAGE", Duration::from_secs(2))?;

    let (x, y) = find_last_text_position(&host, "Retry").expect("retry action");
    host.click(x, y)?;
    host.wait_for_text("MESSAGE_ACTION: 3/retry", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "Regenerate").expect("regenerate action");
    host.click(x, y)?;
    host.wait_for_text("MESSAGE_ACTION: 2/regenerate", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "Copy block").expect("copy block action");
    host.click(x, y)?;
    host.wait_for_text("MESSAGE_ACTION: 1/copy_block:1001", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_text_selection_highlights_copies_selection_and_falls_back() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--text-selection"], 100, 32)?;

    host.wait_for_text("TEXT-SELECTION-COPY", Duration::from_secs(2))?;
    host.wait_for_text("TEXT-SELECTION-COMMAND", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "TEXT-SELECTION-COPY").expect("selection body");
    let before_bg = host.cell_bgcolor(x, y)?;
    let selected_width = "TEXT-SELECTION-COPY".width().min(u16::MAX as usize) as u16;
    host.drag_left(x, y, x.saturating_add(selected_width), y)?;
    host.wait_for_screen(
        |_| host.cell_bgcolor(x, y).is_ok_and(|bg| bg != before_bg),
        Duration::from_secs(2),
    )?;

    host.key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL)?;
    host.wait_for_output(
        osc52_sequence("TEXT-SELECTION-COPY").as_bytes(),
        Duration::from_secs(2),
    )?;

    host.key_with_mods(KeyCode::Esc, KeyModifiers::NONE)?;
    host.click(x, y)?;
    host.key_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL)?;
    host.wait_for_text("MESSAGE_ACTION: 1/copy_block:1001", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_streaming_cancel_button_emits_and_marks_turn_canceled() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--cancel-action"], 100, 28)?;

    host.wait_for_text("CANCEL-STREAMING-MESSAGE", Duration::from_secs(2))?;
    host.wait_for_text("Cancel", Duration::from_secs(2))?;

    let (x, y) = find_text_position(&host, "Cancel").expect("cancel action");
    host.click(x, y)?;

    host.wait_for_text("CANCELLED: 1", Duration::from_secs(2))?;
    host.wait_for_text("canceled", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_block_mapping_renders_each_block_with_target_widget() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--block-mapping"], 100, 50)?;

    for _ in 0..4 {
        host.wheel_up(8, 10)?;
        thread::sleep(Duration::from_millis(20));
    }

    host.wait_for_text("BLOCK-TEXT", Duration::from_secs(2))?;
    host.wait_for_text("Thinking", Duration::from_secs(2))?;
    host.wait_for_text("BLOCK-THINKING", Duration::from_secs(2))?;
    host.wait_for_text("json_tool", Duration::from_secs(2))?;
    host.wait_for_text("[ ] json_tool", Duration::from_secs(2))?;
    host.wait_for_text("count: 2", Duration::from_secs(2))?;
    host.wait_for_text("path: \"src/lib.rs\"", Duration::from_secs(2))?;
    host.wait_for_text("Tool result: call-json (等待中)", Duration::from_secs(2))?;
    host.wait_for_text("Tool result: call-ansi (exit 0)", Duration::from_secs(2))?;
    host.wait_for_text("ANSI-GREEN", Duration::from_secs(2))?;
    host.wait_for_text("MARKDOWN-OUTPUT", Duration::from_secs(2))?;

    for _ in 0..8 {
        host.wheel_down(8, 10)?;
        thread::sleep(Duration::from_millis(20));
    }

    host.wait_for_text("+TOOL-DIFF", Duration::from_secs(2))?;
    host.wait_for_text("Diff: src/main.rs (pending)", Duration::from_secs(2))?;
    host.wait_for_text("+INLINE-DIFF", Duration::from_secs(2))?;
    host.wait_for_text("[ ] BLOCK-TODO-PENDING", Duration::from_secs(2))?;
    host.wait_for_text("[x] BLOCK-TODO-DONE", Duration::from_secs(2))?;
    host.wait_for_text(
        "File: report.txt (file:///tmp/report.txt)",
        Duration::from_secs(2),
    )?;
    host.wait_for_text("Warning: BLOCK-NOTICE", Duration::from_secs(2))?;
    host.wait_for_text("Artifact Code: block-artifact.rs", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_markdown_wraps_to_responsive_bubble_width() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");

    let mut narrow = PtyTestHost::spawn(bin, &["--responsive-layout"], 70, 24)?;
    narrow.wait_for_text("RESPONSIVE-WRAP", Duration::from_secs(2))?;
    narrow.wait_for_text("RESPONSIVE-END", Duration::from_secs(2))?;
    let narrow_snapshot = narrow.screen_snapshot()?;
    assert!(
        !narrow_snapshot
            .iter()
            .any(|line| line.contains("RESPONSIVE-WRAP") && line.contains("RESPONSIVE-END")),
        "narrow layout should wrap the sentinel text:\n{}",
        narrow_snapshot.join("\n")
    );
    narrow.send_ctrl('q')?;

    let mut wide = PtyTestHost::spawn(bin, &["--responsive-layout"], 120, 24)?;
    wide.wait_for_text("RESPONSIVE-WRAP", Duration::from_secs(2))?;
    wide.wait_for_text("RESPONSIVE-END", Duration::from_secs(2))?;
    let wide_snapshot = wide.screen_snapshot()?;
    assert!(
        wide_snapshot
            .iter()
            .any(|line| line.contains("RESPONSIVE-WRAP") && line.contains("RESPONSIVE-END")),
        "wide layout should keep the sentinel text on one line:\n{}",
        wide_snapshot.join("\n")
    );
    wide.send_ctrl('q')?;

    Ok(())
}

#[test]
fn chat_artifact_code_link_opens_text_viewer_window() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--artifact-link"], 100, 28)?;

    host.wait_for_text("Artifact Code: main.rs", Duration::from_secs(2))?;
    let (x, y) = find_text_position(&host, "Artifact Code: main.rs").expect("code link");
    host.click(x, y)?;

    host.wait_for_text("Code: main.rs", Duration::from_secs(2))?;
    host.wait_for_text("Code Artifact: main.rs", Duration::from_secs(2))?;
    host.wait_for_text("CODE-ARTIFACT", Duration::from_secs(2))?;

    let (close_x, close_y) =
        find_close_button_on_title_row(&host, "Code: main.rs").expect("code viewer close button");
    host.click(close_x, close_y)?;
    host.wait_for_screen(
        |snapshot| !snapshot.iter().any(|line| line.contains("CODE-ARTIFACT")),
        Duration::from_secs(2),
    )?;

    host.send_ctrl('q')?;
    Ok(())
}

#[test]
fn chat_artifact_diff_link_opens_colored_diff_viewer_window() -> anyhow::Result<()> {
    let _guard = chat_pty_lock();
    let bin = env!("CARGO_BIN_EXE_snapshot_chat_app");
    let mut host = PtyTestHost::spawn(bin, &["--artifact-link"], 100, 28)?;

    host.wait_for_text("Artifact Diff: main.patch", Duration::from_secs(2))?;
    let (x, y) = find_text_position(&host, "Artifact Diff: main.patch").expect("diff link");
    host.click(x, y)?;

    host.wait_for_text("Diff: main.patch", Duration::from_secs(2))?;
    host.wait_for_text("Diff Artifact: main.patch", Duration::from_secs(2))?;
    host.wait_for_text("+    println!(\"DIFF-ARTIFACT\");", Duration::from_secs(2))?;
    host.wait_for_text("-    println!(\"old\");", Duration::from_secs(2))?;

    host.send_ctrl('q')?;
    Ok(())
}
