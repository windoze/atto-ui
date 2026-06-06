use atto_ui::composable::{
    Component, ComponentContext, MouseCoordinateSpace, ScrollbarHost, TabMode,
};
use atto_ui::theme::Theme;
use atto_ui::wm::WindowId;
use atto_ui_terminal::TerminalEmulator;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
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
    }
}

fn key_input_after_output(output: &str, key: KeyCode) -> Vec<u8> {
    let terminal = TerminalEmulator::new();
    let handle = terminal.handle();
    handle.process_output_str(output);
    handle.send_event(&Event::Key(KeyEvent::new(key, KeyModifiers::NONE)));
    handle.take_input()
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
    MouseEvent {
        kind,
        column: 2,
        row: 3,
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
