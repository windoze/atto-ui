use std::time::Duration;

use atto_ui_test_host::{KeyCode, KeyModifiers, PtyTestHost};

#[test]
fn pty_typeahead_filters_accepts_and_closes_popup() {
    let bin = env!("CARGO_BIN_EXE_snapshot_typeahead_app");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24).expect("spawn PTY app");

    host.wait_for_text("Command Palette", Duration::from_secs(2))
        .expect("command palette visible");
    host.wait_for_text("/open-file", Duration::from_secs(2))
        .expect("initial command popup visible");

    host.key_with_mods(KeyCode::Esc, KeyModifiers::NONE)
        .expect("close popup with Esc");
    host.wait_for_screen(
        |rows| {
            let screen = rows.join("\n");
            screen.contains("Command Palette") && !screen.contains("/open-file")
        },
        Duration::from_secs(2),
    )
    .expect("Esc hides the popup");

    host.send_str("/").expect("type slash command trigger");
    host.wait_for_text("/open-file", Duration::from_secs(2))
        .expect("slash input reopens popup");
    host.wait_for_text("/search-files", Duration::from_secs(2))
        .expect("slash popup includes second command");

    host.key_with_mods(KeyCode::Down, KeyModifiers::NONE)
        .expect("select next command");
    host.key_with_mods(KeyCode::Enter, KeyModifiers::NONE)
        .expect("accept selected command");
    host.wait_for_text("Accepted: /search-files", Duration::from_secs(2))
        .expect("Enter accepts selected suggestion");

    host.send_ctrl('u').expect("clear accepted query");
    host.send_str("@ta").expect("type file reference query");
    host.wait_for_screen(
        |rows| {
            let screen = rows.join("\n");
            screen.contains("@src/widgets/typeahead.rs") && !screen.contains("/open-file")
        },
        Duration::from_secs(2),
    )
    .expect("fuzzy filter narrows to matching file reference");

    host.key_with_mods(KeyCode::Esc, KeyModifiers::NONE)
        .expect("close filtered popup");
    host.wait_for_screen(
        |rows| {
            let screen = rows.join("\n");
            screen.contains("@ta") && !screen.contains("@src/widgets/typeahead.rs")
        },
        Duration::from_secs(2),
    )
    .expect("Esc closes filtered popup without clearing query");

    host.send_ctrl('q').expect("quit");
    host.wait_for_exit(Duration::from_secs(2))
        .expect("clean exit");
}
