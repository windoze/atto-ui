use atto_ui::composable::{
    Component, ComponentAction, ComponentContext, EventHandling, MouseCoordinateSpace, Scrollable,
    ScrollbarHost, TabMode,
};
use atto_ui::theme::Theme;
use atto_ui::wm::WindowId;
use atto_ui_terminal::{
    TerminalEmulator, TerminalPrefixBinding, TerminalPrefixCommand, TerminalSelectionPosition,
    TerminalShortcut,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

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
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str(output);
    handle.send_event(&Event::Key(KeyEvent::new(key, KeyModifiers::NONE)));
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
    let mut terminal = TerminalEmulator::new();
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
fn terminal_capture_on_click_forwards_the_recapture_click() {
    let theme = Theme::dark();
    let mut terminal = TerminalEmulator::new();
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
    let mut terminal = TerminalEmulator::new();
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
    let mut terminal = TerminalEmulator::new();
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
    let mut terminal = TerminalEmulator::new();
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
    let mut terminal = TerminalEmulator::new();
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
