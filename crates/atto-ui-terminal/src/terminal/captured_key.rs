//! Captured-key input handlers: copy-mode, prefix-command, and command-block
//! navigation key routing while the terminal holds input capture.

use super::*;

pub(crate) enum CapturedKeyAction {
    Consumed,
    Dispatch(Vec<u8>),
    Component(ComponentAction),
    SystemClipboardCopy(String),
}

pub(crate) fn handle_captured_key(shared: &mut TerminalShared, event: KeyEvent) -> CapturedKeyAction {
    if shared.copy_mode.is_some() {
        return handle_copy_mode_key(shared, event);
    }
    if shared.release_shortcut.matches(event) {
        shared.set_capture(false);
        // Intentional release: do not auto-restore capture on refocus.
        shared.capture_suspended_by_blur = false;
        return CapturedKeyAction::Consumed;
    }
    if event.kind == KeyEventKind::Release {
        return CapturedKeyAction::Consumed;
    }
    if handle_command_navigation_key(shared, event) {
        return CapturedKeyAction::Consumed;
    }
    if shared.prefix_pending {
        shared.prefix_pending = false;
        if let Some(command) = shared.prefix_command_for_event(event) {
            return handle_prefix_command(shared, command);
        }
        return encode_prefix_fallback(shared, event)
            .map(CapturedKeyAction::Dispatch)
            .unwrap_or(CapturedKeyAction::Consumed);
    }
    if shared.prefix_shortcut.matches(event) {
        shared.prefix_pending = true;
        return CapturedKeyAction::Consumed;
    }
    encode_key_event(shared.parser.screen(), event)
        .map(CapturedKeyAction::Dispatch)
        .unwrap_or(CapturedKeyAction::Consumed)
}

pub(crate) fn handle_command_navigation_key(shared: &mut TerminalShared, event: KeyEvent) -> bool {
    if event.kind == KeyEventKind::Release || event.modifiers != KeyModifiers::CONTROL {
        return false;
    }
    match event.code {
        KeyCode::Up => shared.scroll_to_previous_command_block().is_some(),
        KeyCode::Down => shared.scroll_to_next_command_block().is_some(),
        _ => false,
    }
}

pub(crate) fn handle_copy_mode_key(shared: &mut TerminalShared, event: KeyEvent) -> CapturedKeyAction {
    if event.kind == KeyEventKind::Release {
        return CapturedKeyAction::Consumed;
    }
    match event.code {
        KeyCode::Esc => {
            shared.cancel_copy_mode();
        }
        KeyCode::Enter => {
            if let Some(text) = shared.finish_copy_mode_copy() {
                return CapturedKeyAction::SystemClipboardCopy(text);
            }
        }
        KeyCode::Up => {
            let _ = shared.move_copy_mode_cursor(-1, 0);
        }
        KeyCode::Down => {
            let _ = shared.move_copy_mode_cursor(1, 0);
        }
        KeyCode::Left => {
            let _ = shared.move_copy_mode_cursor(0, -1);
        }
        KeyCode::Right => {
            let _ = shared.move_copy_mode_cursor(0, 1);
        }
        KeyCode::PageUp => {
            let _ = shared.move_copy_mode_cursor_by_page(-1);
        }
        KeyCode::PageDown => {
            let _ = shared.move_copy_mode_cursor_by_page(1);
        }
        KeyCode::Home => {
            let _ = shared.move_copy_mode_cursor_to_column(0);
        }
        KeyCode::End => {
            let cols = shared.parser.screen().size().1;
            let _ = shared.move_copy_mode_cursor_to_column(cols);
        }
        KeyCode::Char(ch) if event.modifiers == KeyModifiers::NONE => {
            match ch.to_ascii_lowercase() {
                'q' => shared.cancel_copy_mode(),
                'v' | ' ' => shared.begin_copy_mode_selection(),
                'y' => {
                    if let Some(text) = shared.finish_copy_mode_copy() {
                        return CapturedKeyAction::SystemClipboardCopy(text);
                    }
                }
                'h' => {
                    let _ = shared.move_copy_mode_cursor(0, -1);
                }
                'j' => {
                    let _ = shared.move_copy_mode_cursor(1, 0);
                }
                'k' => {
                    let _ = shared.move_copy_mode_cursor(-1, 0);
                }
                'l' => {
                    let _ = shared.move_copy_mode_cursor(0, 1);
                }
                _ => {}
            }
        }
        _ => {}
    }
    CapturedKeyAction::Consumed
}

pub(crate) fn handle_prefix_command(
    shared: &mut TerminalShared,
    command: TerminalPrefixCommand,
) -> CapturedKeyAction {
    match command {
        TerminalPrefixCommand::ActivateMenu => {
            CapturedKeyAction::Component(ComponentAction::ActivateMenu)
        }
        TerminalPrefixCommand::ToggleWindowManagement => {
            CapturedKeyAction::Component(ComponentAction::ToggleWindowManagement)
        }
        TerminalPrefixCommand::ToggleMaximize => {
            CapturedKeyAction::Component(ComponentAction::ToggleMaximizeWindow)
        }
        TerminalPrefixCommand::EnterCopyMode => {
            shared.enter_copy_mode();
            CapturedKeyAction::Consumed
        }
        TerminalPrefixCommand::PasteCopyBuffer => shared
            .paste_copy_buffer_bytes()
            .map(CapturedKeyAction::Dispatch)
            .unwrap_or(CapturedKeyAction::Consumed),
        TerminalPrefixCommand::SendPrefix => encode_prefix_literal(shared)
            .map(CapturedKeyAction::Dispatch)
            .unwrap_or(CapturedKeyAction::Consumed),
    }
}

pub(crate) fn encode_prefix_literal(shared: &TerminalShared) -> Option<Vec<u8>> {
    encode_key_event(
        shared.parser.screen(),
        KeyEvent::new(
            shared.prefix_shortcut.code,
            shared.prefix_shortcut.modifiers,
        ),
    )
}

pub(crate) fn encode_prefix_fallback(shared: &TerminalShared, event: KeyEvent) -> Option<Vec<u8>> {
    let screen = shared.parser.screen();
    let mut bytes = encode_key_event(
        screen,
        KeyEvent::new(
            shared.prefix_shortcut.code,
            shared.prefix_shortcut.modifiers,
        ),
    )
    .unwrap_or_default();
    if let Some(mut event_bytes) = encode_key_event(screen, event) {
        bytes.append(&mut event_bytes);
    }
    if bytes.is_empty() { None } else { Some(bytes) }
}
