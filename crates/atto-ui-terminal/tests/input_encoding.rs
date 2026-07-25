use std::sync::{Arc, Mutex};

use atto_ui::composable::{
    Component, ComponentAction, ComponentContext, EventHandling, MouseCoordinateSpace, Scrollable,
    ScrollbarHost, TabMode,
};
use atto_ui::theme::Theme;
use atto_ui::wm::WindowId;
use atto_ui_terminal::{
    TerminalColorSpec, TerminalCommandBlockPresentation, TerminalConfig, TerminalCursorShape,
    TerminalCursorShapeConfig, TerminalEmulator, TerminalPrefixBinding, TerminalPrefixCommand,
    TerminalSelectionPosition, TerminalShortcut, TerminalShortcutConfig, TerminalShortcutModifier,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

#[derive(Clone, Copy)]
enum MouseProtocol {
    PressRelease,
    ButtonMotion,
    AnyMotion,
}

#[derive(Clone, Copy)]
enum MouseEncoding {
    Sgr,
    X10,
}

fn context(theme: &Theme) -> ComponentContext<'_> {
    ComponentContext {
        theme,
        window_id: WindowId::default(),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    }
}

fn key_input_after_output(output: &str, key: KeyCode) -> Vec<u8> {
    key_event_input_after_output(output, KeyEvent::new(key, KeyModifiers::NONE))
}

fn key_event_input_after_output(output: &str, key: KeyEvent) -> Vec<u8> {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str(output);
    handle.send_event(&Event::Key(key));
    handle.take_input()
}

fn component_key_input_with_terminal(mut terminal: TerminalEmulator, keys: &[KeyEvent]) -> Vec<u8> {
    let theme = Theme::dark();
    let handle = terminal.handle();
    for key in keys {
        terminal.handle_event(&Event::Key(*key), context(&theme));
    }
    handle.take_input()
}

fn component_key_input(keys: &[KeyEvent]) -> Vec<u8> {
    component_key_input_with_terminal(TerminalEmulator::new(), keys)
}

fn component_key_actions_with_terminal(
    mut terminal: TerminalEmulator,
    keys: &[KeyEvent],
) -> Vec<ComponentAction> {
    let theme = Theme::dark();
    keys.iter()
        .map(|key| {
            terminal
                .handle_event(&Event::Key(*key), context(&theme))
                .action
        })
        .collect()
}

fn component_key_actions(keys: &[KeyEvent]) -> Vec<ComponentAction> {
    component_key_actions_with_terminal(TerminalEmulator::new(), keys)
}

fn ctrl_key(ch: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
}

fn keypad_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new_with_kind_and_state(
        code,
        KeyModifiers::NONE,
        KeyEventKind::Press,
        KeyEventState::KEYPAD,
    )
}

fn paste_input_after_output(output: &str, text: &str) -> Vec<u8> {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str(output);
    handle.send_event(&Event::Paste(text.to_string()));
    handle.take_input()
}

fn mouse_input(protocol: MouseProtocol, encoding: MouseEncoding, event: MouseEvent) -> Vec<u8> {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str(mouse_mode_sequence(protocol, encoding).as_str());
    handle.send_event(&Event::Mouse(event));
    handle.take_input()
}

fn component_mouse_input_and_selection(
    output: &str,
    events: &[MouseEvent],
) -> (Vec<u8>, Option<String>) {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str(output);

    for event in events {
        let result = terminal.handle_event(&Event::Mouse(*event), context(&theme));
        assert!(result.is_consumed());
    }

    (handle.take_input(), handle.selected_text())
}

fn mouse_mode_sequence(protocol: MouseProtocol, encoding: MouseEncoding) -> String {
    let protocol_seq = match protocol {
        MouseProtocol::PressRelease => "\x1b[?1000h",
        MouseProtocol::ButtonMotion => "\x1b[?1002h",
        MouseProtocol::AnyMotion => "\x1b[?1003h",
    };
    let encoding_seq = match encoding {
        MouseEncoding::Sgr => "\x1b[?1006h",
        MouseEncoding::X10 => "",
    };
    format!("{protocol_seq}{encoding_seq}")
}

fn mouse_event(kind: MouseEventKind, modifiers: KeyModifiers) -> MouseEvent {
    mouse_event_at(kind, 2, 3, modifiers)
}

fn mouse_event_at(
    kind: MouseEventKind,
    column: u16,
    row: u16,
    modifiers: KeyModifiers,
) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers,
    }
}

fn modifier_bits(modifiers: KeyModifiers) -> u16 {
    let mut bits = 0;
    if modifiers.contains(KeyModifiers::SHIFT) {
        bits += 4;
    }
    if modifiers.contains(KeyModifiers::ALT) {
        bits += 8;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        bits += 16;
    }
    bits
}

fn expected_mouse_input(
    protocol: MouseProtocol,
    encoding: MouseEncoding,
    kind: MouseEventKind,
    modifiers: KeyModifiers,
) -> Vec<u8> {
    let base = match kind {
        MouseEventKind::Down(button) => Some(button_code(button)),
        MouseEventKind::Up(button) => Some(match encoding {
            MouseEncoding::Sgr => button_code(button),
            MouseEncoding::X10 => 3,
        }),
        MouseEventKind::Drag(button) => match protocol {
            MouseProtocol::PressRelease => None,
            MouseProtocol::ButtonMotion | MouseProtocol::AnyMotion => {
                Some(32 + button_code(button))
            }
        },
        MouseEventKind::Moved => match protocol {
            MouseProtocol::AnyMotion => Some(35),
            MouseProtocol::PressRelease | MouseProtocol::ButtonMotion => None,
        },
        MouseEventKind::ScrollUp => Some(64),
        MouseEventKind::ScrollDown => Some(65),
        MouseEventKind::ScrollLeft => Some(66),
        MouseEventKind::ScrollRight => Some(67),
    };
    let Some(base) = base else {
        return Vec::new();
    };

    let cb = base + modifier_bits(modifiers);
    match encoding {
        MouseEncoding::Sgr => {
            let suffix = if matches!(kind, MouseEventKind::Up(_)) {
                'm'
            } else {
                'M'
            };
            format!("\x1b[<{cb};3;4{suffix}").into_bytes()
        }
        MouseEncoding::X10 => vec![0x1b, b'[', b'M', (cb + 32) as u8, 35, 36],
    }
}

fn button_code(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

#[test]
fn terminal_utf8_mouse_encoding_uses_multibyte_for_large_coordinates() {
    // DECSET 1005 (UTF-8 mouse encoding) must UTF-8-encode field values > 127
    // as multiple bytes, not clamp them into one byte like legacy X10. On a wide
    // terminal a click past column ~95 produces such a value.
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    // Wide enough that column 200 is on-screen.
    assert!(handle.resize(24, 240));
    // Enable button-event tracking (1002) + UTF-8 encoding (1005).
    handle.process_output_str("\x1b[?1002h\x1b[?1005h");

    let event = mouse_event_at(
        MouseEventKind::Down(MouseButton::Left),
        200,
        3,
        KeyModifiers::NONE,
    );
    let bytes = {
        terminal.handle_event(&Event::Mouse(event), context(&Theme::dark()));
        handle.take_input()
    };

    // cb = 0 + 32 = 32 (single byte). x = 200 + 1 + 32 = 233 → U+00E9 → 0xC3 0xA9.
    // y = 3 + 1 + 32 = 36 (single byte).
    let mut expected = vec![0x1b, b'[', b'M', 32];
    let mut buf = [0u8; 4];
    expected.extend_from_slice('\u{00E9}'.encode_utf8(&mut buf).as_bytes());
    expected.push(36);
    assert_eq!(bytes, expected);
    // The x field must be two bytes here — a clamped single byte would be a bug.
    assert!(bytes.len() > 6);
}

#[test]
fn terminal_mouse_encoding_matrix_covers_protocol_encoding_and_modifiers() {
    let protocols = [
        MouseProtocol::PressRelease,
        MouseProtocol::ButtonMotion,
        MouseProtocol::AnyMotion,
    ];
    let encodings = [MouseEncoding::Sgr, MouseEncoding::X10];
    let modifiers = [
        KeyModifiers::NONE,
        KeyModifiers::SHIFT,
        KeyModifiers::ALT,
        KeyModifiers::CONTROL,
        KeyModifiers::SHIFT | KeyModifiers::ALT,
        KeyModifiers::SHIFT | KeyModifiers::CONTROL,
        KeyModifiers::ALT | KeyModifiers::CONTROL,
        KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL,
    ];
    let kinds = [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Down(MouseButton::Middle),
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Middle),
        MouseEventKind::Up(MouseButton::Right),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Middle),
        MouseEventKind::Drag(MouseButton::Right),
        MouseEventKind::Moved,
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
        MouseEventKind::ScrollLeft,
        MouseEventKind::ScrollRight,
    ];

    for protocol in protocols {
        for encoding in encodings {
            for modifiers in modifiers {
                for kind in kinds {
                    let actual = mouse_input(protocol, encoding, mouse_event(kind, modifiers));
                    let expected = expected_mouse_input(protocol, encoding, kind, modifiers);
                    assert_eq!(actual, expected);
                }
            }
        }
    }
}

#[test]
fn terminal_plain_drag_selects_locally_without_mouse_reporting() {
    let (input, selected) = component_mouse_input_and_selection(
        "alpha beta",
        &[
            mouse_event_at(
                MouseEventKind::Down(MouseButton::Left),
                0,
                0,
                KeyModifiers::NONE,
            ),
            mouse_event_at(
                MouseEventKind::Drag(MouseButton::Left),
                4,
                0,
                KeyModifiers::NONE,
            ),
            mouse_event_at(
                MouseEventKind::Up(MouseButton::Left),
                4,
                0,
                KeyModifiers::NONE,
            ),
        ],
    );

    assert_eq!(input, b"");
    assert_eq!(selected.as_deref(), Some("alpha"));
}

#[test]
fn terminal_plain_click_without_mouse_reporting_does_not_select_text() {
    let (input, selected) = component_mouse_input_and_selection(
        "alpha beta",
        &[
            mouse_event_at(
                MouseEventKind::Down(MouseButton::Left),
                0,
                0,
                KeyModifiers::NONE,
            ),
            mouse_event_at(
                MouseEventKind::Up(MouseButton::Left),
                0,
                0,
                KeyModifiers::NONE,
            ),
        ],
    );

    assert_eq!(input, b"");
    assert_eq!(selected, None);
}

#[test]
fn terminal_mouse_reporting_plain_drag_forwards_to_subprocess() {
    let output = format!(
        "{}alpha beta",
        mouse_mode_sequence(MouseProtocol::ButtonMotion, MouseEncoding::Sgr)
    );
    let events = [
        mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
        mouse_event(MouseEventKind::Drag(MouseButton::Left), KeyModifiers::NONE),
        mouse_event(MouseEventKind::Up(MouseButton::Left), KeyModifiers::NONE),
    ];
    let mut expected = Vec::new();
    for event in events {
        expected.extend(expected_mouse_input(
            MouseProtocol::ButtonMotion,
            MouseEncoding::Sgr,
            event.kind,
            event.modifiers,
        ));
    }

    let (input, selected) = component_mouse_input_and_selection(&output, &events);

    assert_eq!(input, expected);
    assert_eq!(selected, None);
}

#[test]
fn terminal_mouse_reporting_shift_drag_selects_locally() {
    let output = format!(
        "{}alpha beta",
        mouse_mode_sequence(MouseProtocol::ButtonMotion, MouseEncoding::Sgr)
    );
    let (input, selected) = component_mouse_input_and_selection(
        &output,
        &[
            mouse_event_at(
                MouseEventKind::Down(MouseButton::Left),
                0,
                0,
                KeyModifiers::SHIFT,
            ),
            mouse_event_at(
                MouseEventKind::Drag(MouseButton::Left),
                4,
                0,
                KeyModifiers::SHIFT,
            ),
            mouse_event_at(
                MouseEventKind::Up(MouseButton::Left),
                4,
                0,
                KeyModifiers::SHIFT,
            ),
        ],
    );

    assert_eq!(input, b"");
    assert_eq!(selected.as_deref(), Some("alpha"));
}

#[test]
fn terminal_mouse_selection_copies_to_local_buffer_on_release() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str("alpha beta");

    for event in [
        mouse_event_at(
            MouseEventKind::Down(MouseButton::Left),
            0,
            0,
            KeyModifiers::NONE,
        ),
        mouse_event_at(
            MouseEventKind::Drag(MouseButton::Left),
            4,
            0,
            KeyModifiers::NONE,
        ),
        mouse_event_at(
            MouseEventKind::Up(MouseButton::Left),
            4,
            0,
            KeyModifiers::NONE,
        ),
    ] {
        assert!(
            terminal
                .handle_event(&Event::Mouse(event), context(&theme))
                .is_consumed()
        );
    }

    assert_eq!(handle.selected_text().as_deref(), Some("alpha"));
    assert_eq!(handle.copied_text().as_deref(), Some("alpha"));
}

#[test]
fn terminal_mouse_selection_copies_wide_character_from_either_cell() {
    for col in [6, 7] {
        let (input, selected) = component_mouse_input_and_selection(
            "alpha 你",
            &[
                mouse_event_at(
                    MouseEventKind::Down(MouseButton::Left),
                    col,
                    0,
                    KeyModifiers::NONE,
                ),
                mouse_event_at(
                    MouseEventKind::Drag(MouseButton::Left),
                    col,
                    0,
                    KeyModifiers::NONE,
                ),
                mouse_event_at(
                    MouseEventKind::Up(MouseButton::Left),
                    col,
                    0,
                    KeyModifiers::NONE,
                ),
            ],
        );

        assert_eq!(input, b"");
        assert_eq!(selected.as_deref(), Some("你"));
    }
}

#[test]
fn terminal_capture_on_click_forwards_the_recapture_click() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str(
        mouse_mode_sequence(MouseProtocol::ButtonMotion, MouseEncoding::Sgr).as_str(),
    );
    handle.set_capture(false);

    let event = mouse_event(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE);
    let result = terminal.handle_event(&Event::Mouse(event), context(&theme));

    assert!(result.is_consumed());
    assert!(handle.capture());
    assert_eq!(
        handle.take_input(),
        expected_mouse_input(
            MouseProtocol::ButtonMotion,
            MouseEncoding::Sgr,
            event.kind,
            event.modifiers
        )
    );
}

#[test]
fn terminal_bracketed_paste_wraps_only_when_enabled() {
    assert_eq!(paste_input_after_output("", "one\ntwo"), b"one\ntwo");
    assert_eq!(
        paste_input_after_output("\x1b[?2004h", "one\ntwo"),
        b"\x1b[200~one\ntwo\x1b[201~"
    );
}

#[test]
fn terminal_local_copy_buffer_pastes_with_bracketed_paste() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str("alpha beta");
    handle.begin_selection(TerminalSelectionPosition::new(0, 0));
    handle.update_selection(TerminalSelectionPosition::new(0, 5));

    assert_eq!(handle.copy_selection().as_deref(), Some("alpha"));

    handle.process_output_str("\x1b[?2004h");
    assert!(handle.paste_copied_text());
    assert_eq!(handle.take_input(), b"\x1b[200~alpha\x1b[201~");

    terminal.handle_event(&Event::Key(ctrl_key('b')), context(&theme));
    terminal.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE)),
        context(&theme),
    );
    assert_eq!(handle.take_input(), b"\x1b[200~alpha\x1b[201~");
}

#[test]
fn terminal_copy_selection_syncs_configured_system_clipboard() {
    let copied = Arc::new(Mutex::new(Vec::new()));
    let copied_for_clipboard = Arc::clone(&copied);
    let terminal = TerminalEmulator::new().system_clipboard(move |text: &str| {
        copied_for_clipboard
            .lock()
            .expect("clipboard lock")
            .push(text.to_string());
        Ok(())
    });
    let handle = terminal.handle();
    handle.process_output_str("alpha beta");
    handle.begin_selection(TerminalSelectionPosition::new(0, 0));
    handle.update_selection(TerminalSelectionPosition::new(0, 5));

    assert_eq!(handle.copy_selection().as_deref(), Some("alpha"));
    assert_eq!(handle.copied_text().as_deref(), Some("alpha"));
    assert_eq!(
        handle.last_system_clipboard_text().as_deref(),
        Some("alpha")
    );
    assert_eq!(handle.last_system_clipboard_error(), None);
    assert_eq!(copied.lock().expect("clipboard lock").as_slice(), ["alpha"]);
}

#[test]
fn terminal_copy_mode_copy_syncs_configured_system_clipboard() {
    let copied = Arc::new(Mutex::new(Vec::new()));
    let copied_for_clipboard = Arc::clone(&copied);
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().system_clipboard(move |text: &str| {
        copied_for_clipboard
            .lock()
            .expect("clipboard lock")
            .push(text.to_string());
        Ok(())
    });
    let handle = terminal.handle();
    handle.process_output_str("alpha beta");

    for key in [
        ctrl_key('b'),
        KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
    ] {
        terminal.handle_event(&Event::Key(key), context(&theme));
    }

    assert_eq!(handle.copied_text().as_deref(), Some("alpha"));
    assert_eq!(
        handle.last_system_clipboard_text().as_deref(),
        Some("alpha")
    );
    assert_eq!(handle.last_system_clipboard_error(), None);
    assert_eq!(copied.lock().expect("clipboard lock").as_slice(), ["alpha"]);
}

#[test]
fn terminal_application_cursor_changes_arrow_key_encoding() {
    assert_eq!(key_input_after_output("", KeyCode::Up), b"\x1b[A");
    assert_eq!(key_input_after_output("\x1b[?1h", KeyCode::Up), b"\x1bOA");
    assert_eq!(key_input_after_output("\x1b[?1h", KeyCode::Down), b"\x1bOB");
    assert_eq!(
        key_input_after_output("\x1b[?1h", KeyCode::Right),
        b"\x1bOC"
    );
    assert_eq!(key_input_after_output("\x1b[?1h", KeyCode::Left), b"\x1bOD");
}

#[test]
fn terminal_application_keypad_changes_keypad_key_encoding() {
    assert_eq!(
        key_event_input_after_output("", keypad_key(KeyCode::Char('1'))),
        b"1"
    );
    assert_eq!(
        key_event_input_after_output("\x1b=", keypad_key(KeyCode::Char('0'))),
        b"\x1bOp"
    );
    assert_eq!(
        key_event_input_after_output("\x1b=", keypad_key(KeyCode::Char('1'))),
        b"\x1bOq"
    );
    assert_eq!(
        key_event_input_after_output("\x1b=", keypad_key(KeyCode::Char('5'))),
        b"\x1bOu"
    );
    assert_eq!(
        key_event_input_after_output("\x1b=", keypad_key(KeyCode::Char('9'))),
        b"\x1bOy"
    );
    assert_eq!(
        key_event_input_after_output("\x1b=", keypad_key(KeyCode::Char('.'))),
        b"\x1bOn"
    );
    assert_eq!(
        key_event_input_after_output("\x1b=", keypad_key(KeyCode::Enter)),
        b"\x1bOM"
    );
}

#[test]
fn terminal_application_keypad_uses_keypad_origin_only() {
    assert_eq!(key_input_after_output("\x1b=", KeyCode::Char('1')), b"1");
    assert_eq!(key_input_after_output("\x1b=", KeyCode::Up), b"\x1b[A");
    assert_eq!(
        key_event_input_after_output("\x1b=\x1b>", keypad_key(KeyCode::Char('1'))),
        b"1"
    );
    assert_eq!(
        key_event_input_after_output(
            "\x1b=",
            KeyEvent::new(KeyCode::KeypadBegin, KeyModifiers::NONE),
        ),
        b"\x1bOE"
    );
}

#[test]
fn terminal_application_keypad_encodes_keypad_operator_and_navigation_keys() {
    let cases = [
        (KeyCode::Char('/'), b"\x1bOo".as_slice()),
        (KeyCode::Char('*'), b"\x1bOj"),
        (KeyCode::Char('+'), b"\x1bOk"),
        (KeyCode::Char('-'), b"\x1bOm"),
        (KeyCode::Char(','), b"\x1bOl"),
        (KeyCode::Char('='), b"\x1bOX"),
        (KeyCode::Home, b"\x1bOw"),
        (KeyCode::Up, b"\x1bOx"),
        (KeyCode::PageUp, b"\x1bOy"),
        (KeyCode::Left, b"\x1bOt"),
        (KeyCode::Right, b"\x1bOv"),
        (KeyCode::End, b"\x1bOq"),
        (KeyCode::Down, b"\x1bOr"),
        (KeyCode::PageDown, b"\x1bOs"),
        (KeyCode::Insert, b"\x1bOp"),
        (KeyCode::Delete, b"\x1bOn"),
        (KeyCode::KeypadBegin, b"\x1bOE"),
    ];

    for (code, expected) in cases {
        assert_eq!(
            key_event_input_after_output("\x1b=", keypad_key(code)),
            expected,
            "keypad {code:?}"
        );
    }
}

#[test]
fn terminal_prefix_key_waits_for_next_captured_key() {
    assert_eq!(component_key_input(&[ctrl_key('b')]), b"");
    assert_eq!(
        component_key_input(&[
            ctrl_key('b'),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        ]),
        b"\x02x"
    );
}

#[test]
fn terminal_prefix_command_table_maps_shell_commands_to_component_actions() {
    assert_eq!(
        component_key_actions(&[
            ctrl_key('b'),
            KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE)
        ]),
        vec![ComponentAction::None, ComponentAction::ActivateMenu]
    );
    assert_eq!(
        component_key_actions(&[
            ctrl_key('b'),
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
        ]),
        vec![
            ComponentAction::None,
            ComponentAction::ToggleWindowManagement
        ]
    );
    assert_eq!(
        component_key_actions(&[
            ctrl_key('b'),
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        ]),
        vec![ComponentAction::None, ComponentAction::ToggleMaximizeWindow]
    );
    assert_eq!(
        component_key_input(&[
            ctrl_key('b'),
            KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE),
            ctrl_key('b'),
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
            ctrl_key('b'),
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        ]),
        b""
    );
}

#[test]
fn terminal_prefix_command_table_can_be_replaced() {
    let bindings = [TerminalPrefixBinding::new(
        TerminalShortcut::new(KeyCode::Char('m'), KeyModifiers::NONE),
        TerminalPrefixCommand::ToggleMaximize,
    )];
    let terminal = TerminalEmulator::new().prefix_bindings(bindings);

    assert_eq!(
        component_key_actions_with_terminal(
            terminal,
            &[
                ctrl_key('b'),
                KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE),
            ],
        ),
        vec![ComponentAction::None, ComponentAction::ToggleMaximizeWindow]
    );

    let fallback = component_key_input_with_terminal(
        TerminalEmulator::new().prefix_bindings(bindings),
        &[
            ctrl_key('b'),
            KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE),
        ],
    );
    assert!(fallback.starts_with(b"\x02"));
    assert!(fallback.len() > 1);
}

#[test]
fn terminal_prefix_command_table_binding_replaces_existing_command() {
    let terminal = TerminalEmulator::new().prefix_binding(
        TerminalShortcut::new(KeyCode::Char('W'), KeyModifiers::NONE),
        TerminalPrefixCommand::ActivateMenu,
    );

    assert_eq!(
        component_key_actions_with_terminal(
            terminal,
            &[
                ctrl_key('b'),
                KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
            ],
        ),
        vec![ComponentAction::None, ComponentAction::ActivateMenu]
    );
}

#[test]
fn terminal_prefix_command_table_runtime_replacement_clears_pending_prefix() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();

    terminal.handle_event(&Event::Key(ctrl_key('b')), context(&theme));
    handle.set_prefix_bindings([TerminalPrefixBinding::new(
        TerminalShortcut::new(KeyCode::Char('m'), KeyModifiers::NONE),
        TerminalPrefixCommand::ToggleMaximize,
    )]);

    terminal.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
        context(&theme),
    );

    assert_eq!(handle.take_input(), b"x");
}

#[test]
fn terminal_prefix_command_table_enters_copy_mode() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str("alpha");

    terminal.handle_event(&Event::Key(ctrl_key('b')), context(&theme));
    let result = terminal.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE)),
        context(&theme),
    );

    assert!(result.is_consumed());
    assert_eq!(result.action, ComponentAction::None);
    assert!(handle.copy_mode());
    assert_eq!(
        handle.copy_mode_cursor(),
        Some(TerminalSelectionPosition::new(0, 5))
    );
    assert_eq!(handle.take_input(), b"");
}

#[test]
fn terminal_copy_mode_selects_and_copies_with_vi_keys() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str("alpha beta");

    for key in [
        ctrl_key('b'),
        KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
    ] {
        terminal.handle_event(&Event::Key(key), context(&theme));
    }

    assert!(!handle.copy_mode());
    assert_eq!(handle.copied_text().as_deref(), Some("alpha"));
    assert_eq!(handle.selection_range(), None);
    assert_eq!(handle.take_input(), b"");
}

#[test]
fn terminal_copy_mode_selects_and_copies_with_arrow_keys_and_enter() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str("alpha beta");

    for key in [
        ctrl_key('b'),
        KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    ] {
        terminal.handle_event(&Event::Key(key), context(&theme));
    }

    assert!(!handle.copy_mode());
    assert_eq!(handle.copied_text().as_deref(), Some("alpha"));
    assert_eq!(handle.take_input(), b"");
}

#[test]
fn terminal_copy_mode_cancel_clears_selection_without_forwarding() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str("alpha beta");

    for key in [
        ctrl_key('b'),
        KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
    ] {
        terminal.handle_event(&Event::Key(key), context(&theme));
    }

    assert!(!handle.copy_mode());
    assert_eq!(handle.selected_text(), None);
    assert_eq!(handle.copied_text(), None);
    assert_eq!(handle.take_input(), b"");
}

#[test]
fn terminal_copy_mode_q_cancels_without_forwarding() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    terminal.handle_event(&Event::Key(ctrl_key('b')), context(&theme));
    terminal.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE)),
        context(&theme),
    );
    terminal.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        context(&theme),
    );

    assert!(!handle.copy_mode());
    assert_eq!(handle.take_input(), b"");
}

#[test]
fn terminal_copy_mode_wheel_stays_local_even_with_mouse_reporting() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str(
        format!(
            "{}alpha beta",
            mouse_mode_sequence(MouseProtocol::ButtonMotion, MouseEncoding::Sgr)
        )
        .as_str(),
    );

    terminal.handle_event(&Event::Key(ctrl_key('b')), context(&theme));
    terminal.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE)),
        context(&theme),
    );
    let wheel = mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE);
    let result = terminal.handle_event(&Event::Mouse(wheel), context(&theme));

    assert!(result.is_consumed());
    assert!(handle.copy_mode());
    assert_eq!(handle.take_input(), b"");
}

#[test]
fn terminal_alternate_screen_wheel_sends_direction_keys() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str("\x1b[?1049h");

    let up = terminal.handle_event(
        &Event::Mouse(mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE)),
        context(&theme),
    );
    assert!(up.is_consumed());
    assert_eq!(handle.take_input(), b"\x1b[A\x1b[A\x1b[A");

    let down = terminal.handle_event(
        &Event::Mouse(mouse_event(MouseEventKind::ScrollDown, KeyModifiers::NONE)),
        context(&theme),
    );
    assert!(down.is_consumed());
    assert_eq!(handle.take_input(), b"\x1b[B\x1b[B\x1b[B");
}

#[test]
fn terminal_config_applies_shortcuts_scrollback_and_alt_scroll() {
    let theme = Theme::dark();
    let mut config = TerminalConfig {
        scrollback_len: 7,
        prefix_key: TerminalShortcutConfig::control_letter('a'),
        release_shortcut: TerminalShortcutConfig::new("g", [TerminalShortcutModifier::Control]),
        ..TerminalConfig::default()
    };
    config.alternate_screen_scroll.step = 2;
    config.alternate_screen_scroll.scroll_up_key = TerminalShortcutConfig::new("pageup", []);
    config.alternate_screen_scroll.scroll_down_key = TerminalShortcutConfig::new("pagedown", []);

    let mut terminal = TerminalEmulator::from_config(&config).expect("configured terminal");
    let handle = terminal.handle();

    assert_eq!(handle.scrollback_len(), 7);
    assert_eq!(
        handle.prefix_shortcut(),
        TerminalShortcut::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
    );
    assert_eq!(
        handle.release_shortcut(),
        TerminalShortcut::new(KeyCode::Char('g'), KeyModifiers::CONTROL)
    );

    handle.process_output_str("\x1b[?1049h");
    let up = terminal.handle_event(
        &Event::Mouse(mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE)),
        context(&theme),
    );
    assert!(up.is_consumed());
    assert_eq!(handle.take_input(), b"\x1b[5~\x1b[5~");

    let release = terminal.handle_event(&Event::Key(ctrl_key('g')), context(&theme));
    assert!(release.is_consumed());
    assert!(!handle.capture());
}

#[test]
fn terminal_apply_config_updates_live_terminal() {
    let mut config = TerminalConfig {
        prefix_key: TerminalShortcutConfig::control_letter('z'),
        ..TerminalConfig::default()
    };
    config.alternate_screen_scroll.enabled = false;
    config.palette.ansi[1] = TerminalColorSpec::new("#123456");
    config.cursor.default_shape = TerminalCursorShapeConfig::Underline;
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    handle.apply_config(&config).expect("apply config");
    assert_eq!(
        handle.prefix_shortcut(),
        TerminalShortcut::new(KeyCode::Char('z'), KeyModifiers::CONTROL)
    );

    let theme = Theme::dark();
    handle.process_output_str("\x1b[?1049h");
    let result = terminal.handle_event(
        &Event::Mouse(mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE)),
        context(&theme),
    );
    assert!(result.is_consumed());
    assert_eq!(handle.take_input(), b"");

    handle.process_output_str("\x1b[?1049l\x1b[31mR\x1b[mD");
    let mut draw_terminal = Terminal::new(TestBackend::new(4, 1)).expect("test terminal");
    draw_terminal
        .draw(|f| terminal.draw(f, Rect::new(0, 0, 4, 1), context(&theme)))
        .expect("draw after live config apply");
    let red = draw_terminal
        .backend()
        .buffer()
        .cell((0, 0))
        .expect("red cell");
    assert_eq!(red.fg, Color::Rgb(0x12, 0x34, 0x56));
    let cursor = draw_terminal
        .backend()
        .buffer()
        .cell((2, 0))
        .expect("cursor cell");
    assert!(cursor.modifier.contains(Modifier::UNDERLINED));
    assert!(!cursor.modifier.contains(Modifier::REVERSED));
    assert_eq!(handle.cursor_shape(), TerminalCursorShape::Underline);

    assert_eq!(
        component_key_input_with_terminal(
            terminal,
            &[
                ctrl_key('b'),
                ctrl_key('z'),
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            ],
        ),
        b"\x02\x1ax"
    );
}

#[test]
fn terminal_mouse_reporting_wheel_takes_priority_over_alternate_screen_scroll() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str(
        format!(
            "\x1b[?1049h{}",
            mouse_mode_sequence(MouseProtocol::PressRelease, MouseEncoding::Sgr)
        )
        .as_str(),
    );
    let event = mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE);

    let result = terminal.handle_event(&Event::Mouse(event), context(&theme));

    assert!(result.is_consumed());
    assert_eq!(
        handle.take_input(),
        expected_mouse_input(
            MouseProtocol::PressRelease,
            MouseEncoding::Sgr,
            event.kind,
            event.modifiers
        )
    );
}

#[test]
fn terminal_main_screen_wheel_stays_on_local_scrollback() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new().scrollback_len(20);
    let handle = terminal.handle();
    let output = (0..40)
        .map(|line| format!("line-{line:02}\r\n"))
        .collect::<String>();
    handle.process_output_str(&output);
    let before = terminal.scroll_offset().1;

    let result = terminal.handle_event(
        &Event::Mouse(mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE)),
        context(&theme),
    );
    let after = terminal.scroll_offset().1;

    assert!(result.is_consumed());
    assert!(after < before);
    assert_eq!(handle.take_input(), b"");
}

#[test]
fn terminal_large_scroll_step_wheel_does_not_panic() {
    // A scroll step near u16::MAX previously overflowed `step as i16` (and
    // negating i16::MIN panics in debug). Scrolling must stay well-defined and
    // clamp to the available scrollback.
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new()
        .scrollback_len(20)
        .scroll_step(u16::MAX);
    let handle = terminal.handle();
    let output = (0..40)
        .map(|line| format!("line-{line:02}\r\n"))
        .collect::<String>();
    handle.process_output_str(&output);
    let live_offset = terminal.scroll_offset().1;

    // Scroll up (into history) then back down; neither may panic.
    let up = terminal.handle_event(
        &Event::Mouse(mouse_event(MouseEventKind::ScrollUp, KeyModifiers::NONE)),
        context(&theme),
    );
    assert!(up.is_consumed());
    // A huge step scrolls all the way into history (clamped, no overflow).
    assert!(terminal.scroll_offset().1 < live_offset);

    let down = terminal.handle_event(
        &Event::Mouse(mouse_event(MouseEventKind::ScrollDown, KeyModifiers::NONE)),
        context(&theme),
    );
    assert!(down.is_consumed());
    // Scrolled all the way back to the live view.
    assert_eq!(terminal.scroll_offset().1, live_offset);
}

#[test]
fn terminal_prefix_escape_dispatches_single_literal_prefix() {
    assert_eq!(
        component_key_input(&[ctrl_key('b'), ctrl_key('b')]),
        b"\x02"
    );
    let terminal = TerminalEmulator::new()
        .prefix_key('a')
        .expect("valid prefix key");
    assert_eq!(
        component_key_input_with_terminal(terminal, &[ctrl_key('a'), ctrl_key('a')]),
        b"\x01"
    );
}

#[test]
fn terminal_prefix_key_can_be_configured_to_plain_ctrl_letter() {
    let terminal = TerminalEmulator::new()
        .prefix_key('a')
        .expect("valid prefix key");

    assert_eq!(
        component_key_input_with_terminal(
            terminal,
            &[
                ctrl_key('b'),
                ctrl_key('a'),
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            ],
        ),
        b"\x02\x01x"
    );
}

#[test]
fn terminal_ctrl_space_sends_nul() {
    // Ctrl+Space (and Ctrl+@) send NUL (0x00), matching xterm.
    assert_eq!(
        key_event_input_after_output("", KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL)),
        b"\x00"
    );
    assert_eq!(
        key_event_input_after_output("", KeyEvent::new(KeyCode::Char('@'), KeyModifiers::CONTROL)),
        b"\x00"
    );
}

#[test]
fn terminal_prefix_shortcut_normalizes_configured_letter_case() {
    let terminal = TerminalEmulator::new()
        .prefix_shortcut(TerminalShortcut::new(
            KeyCode::Char('Z'),
            KeyModifiers::CONTROL,
        ))
        .expect("valid prefix shortcut");
    let handle = terminal.handle();

    assert_eq!(
        handle.prefix_shortcut(),
        TerminalShortcut::new(KeyCode::Char('z'), KeyModifiers::CONTROL)
    );
    assert_eq!(
        component_key_input_with_terminal(
            terminal,
            &[
                ctrl_key('z'),
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            ],
        ),
        b"\x1ax"
    );
}

#[test]
fn terminal_prefix_shortcut_rejects_non_plain_ctrl_letters() {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    assert!(
        handle
            .set_prefix_shortcut(TerminalShortcut::new(
                KeyCode::Char('1'),
                KeyModifiers::CONTROL,
            ))
            .is_err()
    );
    assert!(
        handle
            .set_prefix_shortcut(TerminalShortcut::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ))
            .is_err()
    );
    assert!(
        handle
            .set_prefix_shortcut(TerminalShortcut::new(KeyCode::F(1), KeyModifiers::CONTROL,))
            .is_err()
    );
    assert_eq!(
        handle.prefix_shortcut(),
        TerminalShortcut::new(KeyCode::Char('b'), KeyModifiers::CONTROL)
    );
}

#[test]
fn terminal_prefix_state_ignores_release_before_fallback_key() {
    assert_eq!(
        component_key_input(&[
            ctrl_key('b'),
            KeyEvent::new_with_kind(
                KeyCode::Char('b'),
                KeyModifiers::CONTROL,
                KeyEventKind::Release,
            ),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        ]),
        b"\x02x"
    );
}

#[test]
fn terminal_prefix_state_applies_to_tab_capture_hook() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    terminal.handle_event(&Event::Key(ctrl_key('b')), context(&theme));
    terminal.handle_event_capture(
        &Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        context(&theme),
    );
    assert_eq!(handle.take_input(), b"\x02\t");
}

#[test]
fn terminal_release_capture_clears_pending_prefix() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new();
    let handle = terminal.handle();

    terminal.handle_event(&Event::Key(ctrl_key('b')), context(&theme));
    terminal.handle_event(
        &Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )),
        context(&theme),
    );
    assert!(!handle.capture());

    handle.set_capture(true);
    terminal.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
        context(&theme),
    );
    assert_eq!(handle.take_input(), b"x");
}

#[test]
fn terminal_draw_resize_updates_parser_snapshot_size() {
    let theme = Theme::dark();
    let mut widget = TerminalEmulator::new();
    let handle = widget.handle();
    let mut terminal = Terminal::new(TestBackend::new(50, 20)).expect("terminal");

    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 20, 5), context(&theme)))
        .expect("draw initial size");
    let initial = handle.snapshot();
    assert_eq!((initial.cols, initial.rows), (20, 5));

    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 35, 8), context(&theme)))
        .expect("draw resized area");
    let resized = handle.snapshot();
    assert_eq!((resized.cols, resized.rows), (35, 8));
}

#[test]
fn terminal_cursor_shape_sequences_update_rendered_cursor() {
    let theme = Theme::dark();
    let mut widget = TerminalEmulator::new();
    let handle = widget.handle();
    let mut terminal = Terminal::new(TestBackend::new(4, 2)).expect("terminal");

    handle.process_output_str("\x1b[2 q");
    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 4, 2), context(&theme)))
        .expect("draw block cursor");
    let block = terminal
        .backend()
        .buffer()
        .cell((0, 0))
        .expect("block cell");
    assert_eq!(handle.cursor_shape(), TerminalCursorShape::Block);
    // Block cursors paint the theme's terminal cursor color as the cell
    // background with a contrasting glyph (instead of a bare REVERSED).
    assert_eq!(block.bg, theme.terminal.cursor);
    assert_eq!(block.fg, theme.terminal.cursor_text);

    handle.process_output_str("\x1b[4 q");
    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 4, 2), context(&theme)))
        .expect("draw underline cursor");
    let underline = terminal
        .backend()
        .buffer()
        .cell((0, 0))
        .expect("underline cell");
    assert_eq!(handle.cursor_shape(), TerminalCursorShape::Underline);
    assert!(underline.modifier.contains(Modifier::UNDERLINED));
    assert!(!underline.modifier.contains(Modifier::REVERSED));

    handle.process_output_str("\x1b[6 q");
    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 4, 2), context(&theme)))
        .expect("draw bar cursor");
    let bar = terminal.backend().buffer().cell((0, 0)).expect("bar cell");
    assert_eq!(handle.cursor_shape(), TerminalCursorShape::Bar);
    assert_eq!(bar.symbol(), "▏");
    // The bar glyph is painted in the cursor color so it stays visible.
    assert_eq!(bar.fg, theme.terminal.cursor);
    assert!(!bar.modifier.contains(Modifier::REVERSED));
    assert!(!bar.modifier.contains(Modifier::UNDERLINED));

    handle.process_output_str("\x1b[0 q");
    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 4, 2), context(&theme)))
        .expect("draw default cursor");
    let default_block = terminal
        .backend()
        .buffer()
        .cell((0, 0))
        .expect("default cell");
    assert_eq!(handle.cursor_shape(), TerminalCursorShape::Block);
    assert_eq!(default_block.bg, theme.terminal.cursor);
    assert_eq!(default_block.fg, theme.terminal.cursor_text);
}

#[test]
fn terminal_config_palette_changes_rendered_colors() {
    let theme = Theme::dark();
    let mut config = TerminalConfig::default();
    config.palette.foreground = Some(TerminalColorSpec::new("#abcdef"));
    config.palette.background = Some(TerminalColorSpec::new("#010203"));
    config.palette.ansi[1] = TerminalColorSpec::new("#123456");
    let mut widget = TerminalEmulator::from_config(&config).expect("configured terminal");
    let handle = widget.handle();
    let mut terminal = Terminal::new(TestBackend::new(4, 1)).expect("terminal");

    handle.process_output_str("\x1b[31mR\x1b[mD");
    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 4, 1), context(&theme)))
        .expect("draw configured colors");

    let red = terminal.backend().buffer().cell((0, 0)).expect("red cell");
    assert_eq!(red.fg, Color::Rgb(0x12, 0x34, 0x56));
    assert_eq!(red.bg, Color::Rgb(0x01, 0x02, 0x03));

    let default = terminal
        .backend()
        .buffer()
        .cell((1, 0))
        .expect("default cell");
    assert_eq!(default.fg, Color::Rgb(0xab, 0xcd, 0xef));
    assert_eq!(default.bg, Color::Rgb(0x01, 0x02, 0x03));
}

#[test]
fn terminal_apply_theme_changes_rendered_default_and_ansi_colors() {
    // A terminal deriving its palette from the theme should re-render in the
    // new theme's colors after apply_theme, without any explicit config.
    let dark = Theme::dark();
    let light = Theme::light();
    let mut widget = TerminalEmulator::new();
    let handle = widget.handle();
    handle.apply_theme(&dark);

    // ANSI green (\x1b[32m) then a default-color cell.
    handle.process_output_str("\x1b[32mG\x1b[mD");

    let mut terminal = Terminal::new(TestBackend::new(4, 1)).expect("terminal");
    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 4, 1), context(&dark)))
        .expect("draw dark");
    let green_dark = terminal.backend().buffer().cell((0, 0)).expect("green").fg;
    let default_dark = terminal
        .backend()
        .buffer()
        .cell((1, 0))
        .expect("default")
        .fg;
    assert_eq!(green_dark, dark.terminal.ansi[2]);
    assert_eq!(default_dark, dark.terminal.foreground);

    // Switch to light and redraw with the light context.
    handle.apply_theme(&light);
    let mut terminal = Terminal::new(TestBackend::new(4, 1)).expect("terminal");
    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 4, 1), context(&light)))
        .expect("draw light");
    let default_light = terminal
        .backend()
        .buffer()
        .cell((1, 0))
        .expect("default")
        .fg;
    let default_light_bg = terminal
        .backend()
        .buffer()
        .cell((1, 0))
        .expect("default")
        .bg;
    assert_eq!(default_light, light.terminal.foreground);
    assert_eq!(default_light_bg, light.terminal.background);
}

#[test]
fn terminal_selection_handle_extracts_text_from_screen() {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str("alpha 你\r\nbeta");

    handle.begin_selection(TerminalSelectionPosition::new(0, 6));
    handle.update_selection(TerminalSelectionPosition::new(1, 2));

    let range = handle.selection_range().expect("selection range");
    assert_eq!(range.start, TerminalSelectionPosition::new(0, 6));
    assert_eq!(range.end, TerminalSelectionPosition::new(1, 2));
    assert_eq!(handle.selected_text().as_deref(), Some("你\nbe"));
    assert!(handle.clear_selection());
    assert_eq!(handle.selected_text(), None);
}

#[test]
fn terminal_selection_position_for_view_cell_uses_visible_scrollback() {
    let theme = Theme::dark();
    let mut widget = TerminalEmulator::new().scrollback_len(10);
    let handle = widget.handle();
    let mut terminal = Terminal::new(TestBackend::new(10, 3)).expect("terminal");

    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 10, 3), context(&theme)))
        .expect("draw initial size");
    handle.process_output_str("one\r\ntwo\r\nthree\r\nfour\r\nfive");
    widget.set_scroll_offset(0, 0);

    let position = handle.selection_position_for_view_cell(1, 2);
    assert_eq!(position, TerminalSelectionPosition::new(1, 2));
}

#[test]
fn terminal_selection_draw_highlights_wide_character_cells() {
    let theme = Theme::dark();
    let mut widget = TerminalEmulator::new();
    let handle = widget.handle();
    let mut terminal = Terminal::new(TestBackend::new(12, 3)).expect("terminal");
    handle.process_output_str("alpha 你");
    handle.begin_selection(TerminalSelectionPosition::new(0, 7));
    handle.update_selection(TerminalSelectionPosition::new(0, 8));

    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 12, 3), context(&theme)))
        .expect("draw selection");

    let buffer = terminal.backend().buffer();
    let selection_bg = theme.selection.bg.expect("selection background");
    assert_ne!(buffer.cell((5, 0)).expect("pre-wide cell").bg, selection_bg);
    assert_eq!(buffer.cell((6, 0)).expect("wide start").bg, selection_bg);
}

#[test]
fn terminal_command_block_presentation_marks_semantic_rows() {
    let theme = Theme::dark();
    let mut widget = TerminalEmulator::new()
        .command_block_presentation(TerminalCommandBlockPresentation::enabled());
    let handle = widget.handle();
    let mut terminal = Terminal::new(TestBackend::new(16, 4)).expect("terminal");

    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 16, 4), context(&theme)))
        .expect("draw initial size");
    handle.process_output_str(
        "\x1b]133;A\x07$ false\x1b]133;B\x07\r\n\
         \x1b]133;C\x07boom\r\n\
         \x1b]133;D;2\x07",
    );
    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 16, 4), context(&theme)))
        .expect("draw command block presentation");

    let buffer = terminal.backend().buffer();
    let output_bg = theme.status_bar.bg.expect("status background");
    let separator_fg = theme.status_bar_key.fg.expect("separator foreground");
    let failure_fg = theme
        .named_style("status-segment-error")
        .expect("failure style")
        .fg
        .expect("failure foreground");

    let separator = buffer.cell((8, 0)).expect("separator cell");
    assert_eq!(separator.symbol(), "─");
    assert_eq!(separator.fg, separator_fg);
    assert_eq!(buffer.cell((0, 1)).expect("output cell").symbol(), "b");
    assert_eq!(buffer.cell((0, 1)).expect("output cell").bg, output_bg);

    let marker = buffer.cell((15, 2)).expect("failure marker");
    assert_eq!(marker.symbol(), "!");
    assert_eq!(marker.fg, failure_fg);
}

#[test]
fn terminal_command_block_actions_use_marker_columns() {
    let terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str(
        "\x1b]133;A\x07$ \x1b]133;B\x07echo hi\r\n\
         \x1b]133;C\x07hi\r\n\
         \x1b]133;D;0\x07",
    );

    let index = handle
        .command_block_index_at_position(TerminalSelectionPosition::new(1, 0))
        .expect("output row belongs to command block");
    assert_eq!(
        handle.copy_command_block_command(index).as_deref(),
        Some("echo hi")
    );
    assert_eq!(
        handle.copy_command_block_output(index).as_deref(),
        Some("hi")
    );
    assert_eq!(handle.copied_text().as_deref(), Some("hi"));
    assert_eq!(
        handle.select_command_block_output(index),
        Some(atto_ui_terminal::TerminalSelectionRange {
            start: TerminalSelectionPosition::new(1, 0),
            end: TerminalSelectionPosition::new(2, 0),
        })
    );
    assert_eq!(handle.selected_text().as_deref(), Some("hi\n"));

    assert!(handle.rerun_command_block(index));
    assert_eq!(handle.take_input(), b"echo hi\n");
}

#[test]
fn terminal_command_block_actions_degrade_without_osc_markers() {
    let terminal = TerminalEmulator::new().without_system_clipboard();
    let handle = terminal.handle();
    handle.process_output_str("plain output\r\nwithout shell integration\r\n");

    assert!(handle.command_blocks().is_empty());
    assert_eq!(
        handle.command_block_index_at_position(TerminalSelectionPosition::new(0, 0)),
        None
    );
    assert_eq!(handle.scroll_to_previous_command_block(), None);
    assert_eq!(handle.scroll_to_next_command_block(), None);
    assert_eq!(handle.select_command_block_output(0), None);
    assert_eq!(handle.copy_command_block_command(0), None);
    assert_eq!(handle.copy_command_block_output(0), None);
    assert!(!handle.rerun_command_block(0));
    assert_eq!(handle.take_input(), b"");
    assert_eq!(handle.copied_text(), None);
}

#[test]
fn terminal_ctrl_arrows_navigate_command_blocks_without_forwarding() {
    let theme = Theme::dark();
    let mut widget = TerminalEmulator::new();
    let handle = widget.handle();
    let mut terminal = Terminal::new(TestBackend::new(24, 4)).expect("terminal");

    terminal
        .draw(|f| widget.draw(f, Rect::new(0, 0, 24, 4), context(&theme)))
        .expect("draw initial size");
    for index in 0..6 {
        handle.process_output_str(&format!(
            "\x1b]133;A\x07$ \x1b]133;B\x07cmd{index}\r\n\
             \x1b]133;C\x07out{index}\r\n\
             \x1b]133;D;0\x07"
        ));
    }
    assert_eq!(widget.scroll_offset().1, 9);

    let result = widget.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL)),
        context(&theme),
    );
    assert!(result.is_consumed());
    assert_eq!(handle.take_input(), b"");
    assert_eq!(widget.scroll_offset().1, 8);

    let result = widget.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL)),
        context(&theme),
    );
    assert!(result.is_consumed());
    assert_eq!(handle.take_input(), b"");
    assert_eq!(widget.scroll_offset().1, 9);
}
