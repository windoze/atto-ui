use std::thread;
use std::time::{Duration, Instant};

use atto_ui_test_host::PtyTestHost;
use unicode_width::UnicodeWidthStr;

fn find_text_pos(screen: &str, needle: &str) -> Option<(usize, usize)> {
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte_idx) = line.find(needle) {
            let col = UnicodeWidthStr::width(&line[..byte_idx]);
            return Some((row, col));
        }
    }
    None
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

#[test]
fn pty_mouse_opens_menu_and_triggers_action() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    // Click "File" in the menu bar to open the menu.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "File").expect("find File title");
    host.click(col as u16, row as u16).expect("click File");
    host.wait_for_text("Menu:", Duration::from_secs(2))
        .expect("menu mode visible");

    // Switch to "Help" by clicking it.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Help").expect("find Help title");
    host.click(col as u16, row as u16).expect("click Help");
    host.wait_for_text("About", Duration::from_secs(2))
        .expect("Help dropdown visible");

    // Click "About" and ensure the modal is opened.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "About").expect("find About item");
    host.click(col as u16, row as u16).expect("click About");
    host.wait_for_text("About modal", Duration::from_secs(2))
        .expect("modal opened");

    host.send(b"\r").expect("close modal");
    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_mouse_click_outside_closes_menu() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "File").expect("find File title");
    host.click(col as u16, row as u16).expect("click File");
    host.wait_for_text("Menu:", Duration::from_secs(2))
        .expect("menu mode visible");

    // Click inside the work area, outside the dropdown. The menu should close without falling
    // through to underlying windows.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Log window").expect("find log view");
    host.click(col as u16, row as u16).expect("click work area");
    host.wait_for_text("F10 Menu", Duration::from_secs(2))
        .expect("back to normal mode");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_mouse_click_status_bar_does_not_change_focus() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Focus: 1", Duration::from_secs(2))
        .expect("widgets focused");

    // Status bar is the last row of the screen.
    host.click(1, 23).expect("click status bar");
    assert_text_absent_for(&host, "Focus: 2", Duration::from_millis(200));

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_mouse_click_default_status_f10_opens_menu() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("F10 Menu", Duration::from_secs(2))
        .expect("default status visible");

    host.click(1, 23).expect("click F10 status item");
    host.wait_for_text("Menu:", Duration::from_secs(2))
        .expect("menu mode visible");

    host.send(b"\x1b").expect("close menu");
    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_mouse_click_submenu_item_triggers_command() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Theme: Dark", Duration::from_secs(2))
        .expect("initial theme");

    // Open File menu.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "File").expect("find File title");
    host.click(col as u16, row as u16).expect("click File");
    host.wait_for_text("Quit", Duration::from_secs(2))
        .expect("file dropdown visible");

    // Open Theme submenu.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Theme").expect("find Theme item");
    host.click(col as u16, row as u16).expect("click Theme");
    host.wait_for_text("Light", Duration::from_secs(2))
        .expect("submenu opened");

    // Click Light and verify theme indicator changed.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Light").expect("find Light item");
    host.click(col as u16, row as u16).expect("click Light");
    host.wait_for_text("Theme: Light", Duration::from_secs(2))
        .expect("theme changed");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_mouse_click_checkbox_toggles() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("[x] Enable feature", Duration::from_secs(2))
        .expect("checkbox visible");

    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "Enable feature").expect("find checkbox");
    host.click(col as u16, row as u16).expect("click checkbox");
    host.wait_for_text("[ ] Enable feature", Duration::from_secs(2))
        .expect("checkbox unchecked");

    host.click(col as u16, row as u16)
        .expect("click checkbox again");
    host.wait_for_text("[x] Enable feature", Duration::from_secs(2))
        .expect("checkbox checked");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_mouse_click_textbox_sets_cursor() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("┌Text", Duration::from_secs(2))
        .expect("textbox visible");

    // Click after "he" in "hello", then type "X" and verify insertion position.
    let screen = host.screen_contents().expect("screen");
    let (row, col) = find_text_pos(&screen, "┌Text").expect("find textbox border");
    host.click(col as u16 + 3, row as u16 + 1)
        .expect("click inside textbox after 'he'");

    host.send_str("X").expect("type");
    host.wait_for_text("heXllo", Duration::from_secs(2))
        .expect("cursor moved and text inserted");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_typing_q_in_textbox_does_not_trigger_global_quit() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("hello", Duration::from_secs(2))
        .expect("textbox initial text");

    host.send_str("q").expect("type q");
    host.wait_for_text("helloq", Duration::from_secs(2))
        .expect("q inserted into textbox");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}

#[test]
fn pty_menu_item_shortcut_char_triggers_command() {
    let bin = env!("CARGO_BIN_EXE_snapshot_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");
    host.wait_for_text("Widgets", Duration::from_secs(2))
        .expect("initial render");

    // Open menu and use the File->Quit shortcut (`q`) to exit.
    host.send_str("\x1b[21~").expect("F10");
    host.wait_for_text("Quit", Duration::from_secs(2))
        .expect("dropdown visible");
    host.send_str("q").expect("quit via shortcut");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
