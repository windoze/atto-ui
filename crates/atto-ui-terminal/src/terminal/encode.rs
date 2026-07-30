//! Rendering and input encoding helpers: mouse-coordinate translation, cell
//! styling, cursor/color resolution, and key/mouse escape-sequence encoding.

use super::*;

pub(crate) fn mouse_coords_local(
    area: Option<Rect>,
    m: MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    let Some(area) = area else {
        return Some((m.column, m.row));
    };

    if area.width == 0 || area.height == 0 {
        return None;
    }

    match coordinate_space {
        MouseCoordinateSpace::Absolute => {
            if m.column >= area.x
                && m.column < area.x.saturating_add(area.width)
                && m.row >= area.y
                && m.row < area.y.saturating_add(area.height)
            {
                return Some((
                    m.column.saturating_sub(area.x),
                    m.row.saturating_sub(area.y),
                ));
            }
        }
        MouseCoordinateSpace::Local => {
            if m.column < area.width && m.row < area.height {
                return Some((m.column, m.row));
            }
        }
    }

    None
}

pub(crate) fn mouse_selection_position(
    shared: &mut TerminalShared,
    area: Option<Rect>,
    event: MouseEvent,
    coordinate_space: MouseCoordinateSpace,
    include_cell: bool,
) -> Option<TerminalSelectionPosition> {
    let (col, row) = mouse_coords_local(area, event, coordinate_space)?;
    let max_scrollback = shared.max_scrollback();
    let screen = shared.parser.screen();
    let scrollback = screen.scrollback();
    let (rows, cols) = screen.size();
    if rows == 0 || cols == 0 || row >= rows || col >= cols {
        return None;
    }

    let cell_start = position_for_view_cell(max_scrollback, scrollback, rows, cols, row, col);
    let include_right_edge = if include_cell {
        match shared.selection.anchor() {
            Some(anchor) => cell_start >= anchor,
            None => true,
        }
    } else {
        false
    };
    let selection_col = if include_right_edge {
        col.saturating_add(1).min(cols)
    } else {
        col
    };

    Some(position_for_view_cell(
        max_scrollback,
        scrollback,
        rows,
        cols,
        row,
        selection_col,
    ))
}

pub(crate) fn cell_style(
    cell: &vt100::Cell,
    base_fg: Option<Color>,
    base_bg: Option<Color>,
    palette: &TerminalPalette,
) -> Style {
    let mut fg = resolve_color(cell.fgcolor(), base_fg, palette);
    let mut bg = resolve_color(cell.bgcolor(), base_bg, palette);
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }

    let mut style = Style::default();
    if let Some(fg) = fg {
        style = style.fg(fg);
    }
    if let Some(bg) = bg {
        style = style.bg(bg);
    }

    let mut mods = Modifier::empty();
    if cell.bold() {
        mods |= Modifier::BOLD;
    }
    if cell.dim() {
        mods |= Modifier::DIM;
    }
    if cell.italic() {
        mods |= Modifier::ITALIC;
    }
    if cell.underline() {
        mods |= Modifier::UNDERLINED;
    }

    style.add_modifier(mods)
}

pub(crate) fn apply_cursor_shape(
    cell: &mut Cell,
    shape: TerminalCursorShape,
    cursor: Color,
    cursor_text: Color,
) {
    match shape {
        TerminalCursorShape::Block => {
            // Paint the cursor color as the cell background with a contrasting
            // glyph, rather than a bare REVERSED, so the cursor color is honored.
            cell.set_style(cell.style().bg(cursor).fg(cursor_text));
        }
        TerminalCursorShape::Underline => {
            cell.set_style(cell.style().fg(cursor).add_modifier(Modifier::UNDERLINED));
        }
        TerminalCursorShape::Bar => {
            // A bare `▏` glyph in the cell's own (often dim) fg is nearly
            // invisible; color it with the cursor color so bar-style cursors
            // (e.g. Claude Code / Ink apps) stay visible.
            cell.set_symbol(CURSOR_BAR_SYMBOL);
            cell.set_style(cell.style().fg(cursor));
            cell.set_skip(false);
        }
    }
}

pub(crate) fn resolve_color(
    color: vt100::Color,
    default: Option<Color>,
    palette: &TerminalPalette,
) -> Option<Color> {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) => Some(palette.color_for_index(i)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

pub(crate) fn encode_key_event(screen: &vt100::Screen, event: KeyEvent) -> Option<Vec<u8>> {
    if event.kind == KeyEventKind::Release {
        return None;
    }

    let mods = event.modifiers;
    let shift = mods.contains(KeyModifiers::SHIFT);
    let alt = mods.contains(KeyModifiers::ALT);
    let ctrl = mods.contains(KeyModifiers::CONTROL);

    let mut out = Vec::new();

    if screen.application_keypad()
        && is_keypad_event(event)
        && let Some(seq) = encode_application_keypad_key(event.code, mods)
    {
        out.extend_from_slice(seq.as_bytes());
        return Some(out);
    }

    match event.code {
        KeyCode::Char(c) => {
            if ctrl {
                if let Some(b) = ctrl_char(c) {
                    out.push(b);
                } else {
                    out.extend_from_slice(c.to_string().as_bytes());
                }
            } else {
                out.extend_from_slice(c.to_string().as_bytes());
            }
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::Enter => {
            out.push(b'\r');
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::Backspace => {
            out.push(0x7f);
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::Tab => {
            if shift {
                out.extend_from_slice(b"\x1b[Z");
            } else {
                out.push(b'\t');
            }
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::BackTab => {
            out.extend_from_slice(b"\x1b[Z");
        }
        KeyCode::Esc => {
            out.push(0x1b);
        }
        KeyCode::Up => {
            out.extend_from_slice(encode_cursor_key(screen, 'A', mods).as_bytes());
        }
        KeyCode::Down => {
            out.extend_from_slice(encode_cursor_key(screen, 'B', mods).as_bytes());
        }
        KeyCode::Right => {
            out.extend_from_slice(encode_cursor_key(screen, 'C', mods).as_bytes());
        }
        KeyCode::Left => {
            out.extend_from_slice(encode_cursor_key(screen, 'D', mods).as_bytes());
        }
        KeyCode::Home => {
            out.extend_from_slice(encode_home_end_key(screen, 'H', mods).as_bytes());
        }
        KeyCode::End => {
            out.extend_from_slice(encode_home_end_key(screen, 'F', mods).as_bytes());
        }
        KeyCode::PageUp => {
            out.extend_from_slice(encode_csi_tilde(5, mods).as_bytes());
        }
        KeyCode::PageDown => {
            out.extend_from_slice(encode_csi_tilde(6, mods).as_bytes());
        }
        KeyCode::Insert => {
            out.extend_from_slice(encode_csi_tilde(2, mods).as_bytes());
        }
        KeyCode::Delete => {
            out.extend_from_slice(encode_csi_tilde(3, mods).as_bytes());
        }
        KeyCode::F(n) => {
            if let Some(seq) = encode_function_key(n, mods) {
                out.extend_from_slice(seq.as_bytes());
            }
        }
        _ => return None,
    }

    Some(out)
}

pub(crate) fn is_keypad_event(event: KeyEvent) -> bool {
    event.state.contains(KeyEventState::KEYPAD) || matches!(event.code, KeyCode::KeypadBegin)
}

pub(crate) fn encode_application_keypad_key(
    code: KeyCode,
    mods: KeyModifiers,
) -> Option<&'static str> {
    if modifier_value(mods) != 1 {
        return None;
    }

    match code {
        KeyCode::Char('0') | KeyCode::Insert => Some("\x1bOp"),
        KeyCode::Char('1') | KeyCode::End => Some("\x1bOq"),
        KeyCode::Char('2') | KeyCode::Down => Some("\x1bOr"),
        KeyCode::Char('3') | KeyCode::PageDown => Some("\x1bOs"),
        KeyCode::Char('4') | KeyCode::Left => Some("\x1bOt"),
        KeyCode::Char('5') => Some("\x1bOu"),
        KeyCode::Char('6') | KeyCode::Right => Some("\x1bOv"),
        KeyCode::Char('7') | KeyCode::Home => Some("\x1bOw"),
        KeyCode::Char('8') | KeyCode::Up => Some("\x1bOx"),
        KeyCode::Char('9') | KeyCode::PageUp => Some("\x1bOy"),
        KeyCode::Char('*') => Some("\x1bOj"),
        KeyCode::Char('+') => Some("\x1bOk"),
        KeyCode::Char(',') => Some("\x1bOl"),
        KeyCode::Char('-') => Some("\x1bOm"),
        KeyCode::Char('.') | KeyCode::Delete => Some("\x1bOn"),
        KeyCode::Char('/') => Some("\x1bOo"),
        KeyCode::Enter => Some("\x1bOM"),
        KeyCode::Char('=') => Some("\x1bOX"),
        KeyCode::KeypadBegin => Some("\x1bOE"),
        _ => None,
    }
}

pub(crate) fn encode_cursor_key(
    screen: &vt100::Screen,
    suffix: char,
    mods: KeyModifiers,
) -> String {
    let mod_value = modifier_value(mods);
    if mod_value == 1 {
        if screen.application_cursor() {
            format!("\x1bO{suffix}")
        } else {
            format!("\x1b[{suffix}")
        }
    } else {
        format!("\x1b[1;{mod_value}{suffix}")
    }
}

pub(crate) fn encode_home_end_key(
    screen: &vt100::Screen,
    suffix: char,
    mods: KeyModifiers,
) -> String {
    let mod_value = modifier_value(mods);
    if mod_value == 1 {
        if screen.application_cursor() {
            format!("\x1bO{suffix}")
        } else {
            format!("\x1b[{suffix}")
        }
    } else {
        format!("\x1b[1;{mod_value}{suffix}")
    }
}

pub(crate) fn encode_csi_tilde(n: u8, mods: KeyModifiers) -> String {
    let mod_value = modifier_value(mods);
    if mod_value == 1 {
        format!("\x1b[{n}~")
    } else {
        format!("\x1b[{n};{mod_value}~")
    }
}

pub(crate) fn encode_function_key(n: u8, mods: KeyModifiers) -> Option<String> {
    let mod_value = modifier_value(mods);
    let seq = match n {
        1 => {
            if mod_value == 1 {
                "\x1bOP".to_string()
            } else {
                format!("\x1b[1;{mod_value}P")
            }
        }
        2 => {
            if mod_value == 1 {
                "\x1bOQ".to_string()
            } else {
                format!("\x1b[1;{mod_value}Q")
            }
        }
        3 => {
            if mod_value == 1 {
                "\x1bOR".to_string()
            } else {
                format!("\x1b[1;{mod_value}R")
            }
        }
        4 => {
            if mod_value == 1 {
                "\x1bOS".to_string()
            } else {
                format!("\x1b[1;{mod_value}S")
            }
        }
        5 => encode_csi_tilde(15, mods),
        6 => encode_csi_tilde(17, mods),
        7 => encode_csi_tilde(18, mods),
        8 => encode_csi_tilde(19, mods),
        9 => encode_csi_tilde(20, mods),
        10 => encode_csi_tilde(21, mods),
        11 => encode_csi_tilde(23, mods),
        12 => encode_csi_tilde(24, mods),
        _ => return None,
    };
    Some(seq)
}

pub(crate) fn modifier_value(mods: KeyModifiers) -> u8 {
    let mut value = 1;
    if mods.contains(KeyModifiers::SHIFT) {
        value += 1;
    }
    if mods.contains(KeyModifiers::ALT) {
        value += 2;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        value += 4;
    }
    value
}

pub(crate) fn ctrl_char(c: char) -> Option<u8> {
    let c = c.to_ascii_uppercase();
    match c {
        // Ctrl+@ and Ctrl+Space both send NUL (0x00), matching xterm.
        '@' | ' ' => Some(0),
        'A'..='Z' => Some((c as u8) - b'A' + 1),
        '[' => Some(27),
        '\\' => Some(28),
        ']' => Some(29),
        '^' => Some(30),
        '_' => Some(31),
        '?' => Some(127),
        _ => None,
    }
}

pub(crate) fn encode_mouse_event(
    screen: &vt100::Screen,
    event: MouseEvent,
    area: Option<Rect>,
    coordinate_space: MouseCoordinateSpace,
) -> Option<Vec<u8>> {
    if matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None) {
        return None;
    }

    let (col, row) = mouse_coords_local(area, event, coordinate_space)?;
    let (rows, cols) = screen.size();
    if row >= rows || col >= cols {
        return None;
    }

    let cb = match event.kind {
        MouseEventKind::Down(button) => match button {
            MouseButton::Left => Some(0),
            MouseButton::Middle => Some(1),
            MouseButton::Right => Some(2),
        },
        MouseEventKind::Up(button) => match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::PressRelease
            | vt100::MouseProtocolMode::ButtonMotion
            | vt100::MouseProtocolMode::AnyMotion => match button {
                MouseButton::Left => Some(0),
                MouseButton::Middle => Some(1),
                MouseButton::Right => Some(2),
            },
            _ => None,
        },
        MouseEventKind::Drag(button) => match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::ButtonMotion | vt100::MouseProtocolMode::AnyMotion => {
                match button {
                    MouseButton::Left => Some(32),
                    MouseButton::Middle => Some(33),
                    MouseButton::Right => Some(34),
                }
            }
            _ => None,
        },
        MouseEventKind::Moved => match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::AnyMotion => Some(35),
            _ => None,
        },
        MouseEventKind::ScrollUp => Some(64),
        MouseEventKind::ScrollDown => Some(65),
        MouseEventKind::ScrollLeft => Some(66),
        MouseEventKind::ScrollRight => Some(67),
    }?;

    let mut modifier_bits: u16 = 0;
    if event.modifiers.contains(KeyModifiers::SHIFT) {
        modifier_bits += 4;
    }
    if event.modifiers.contains(KeyModifiers::ALT) {
        modifier_bits += 8;
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        modifier_bits += 16;
    }
    let cb = cb + modifier_bits;

    let x = col.saturating_add(1);
    let y = row.saturating_add(1);

    match screen.mouse_protocol_encoding() {
        vt100::MouseProtocolEncoding::Sgr => {
            let suffix = match event.kind {
                MouseEventKind::Up(_) => 'm',
                _ => 'M',
            };
            let seq = format!("\x1b[<{cb};{x};{y}{suffix}");
            Some(seq.into_bytes())
        }
        vt100::MouseProtocolEncoding::Default => {
            let cb = if matches!(event.kind, MouseEventKind::Up(_)) {
                3 + modifier_bits
            } else {
                cb
            };
            // Legacy X10 encoding: each field is a single byte `value + 32`,
            // clamped to 255 (coordinates past column/row 223 are unrepresentable).
            let cb = (cb + 32).min(255);
            let x = (x + 32).min(255);
            let y = (y + 32).min(255);
            let mut seq = Vec::with_capacity(6);
            seq.extend_from_slice(b"\x1b[M");
            seq.push(cb as u8);
            seq.push(x as u8);
            seq.push(y as u8);
            Some(seq)
        }
        vt100::MouseProtocolEncoding::Utf8 => {
            let cb = if matches!(event.kind, MouseEventKind::Up(_)) {
                3 + modifier_bits
            } else {
                cb
            };
            // UTF-8 encoding (DECSET 1005): each field is `value + 32` encoded as
            // a UTF-8 code point, so values > 127 span multiple bytes instead of
            // being clamped into one. This keeps coordinates correct on terminals
            // wider/taller than ~95 cells.
            let mut seq = Vec::with_capacity(9);
            seq.extend_from_slice(b"\x1b[M");
            push_mouse_utf8_field(&mut seq, cb + 32);
            push_mouse_utf8_field(&mut seq, x + 32);
            push_mouse_utf8_field(&mut seq, y + 32);
            Some(seq)
        }
    }
}

/// Appends a single UTF-8 mouse-report field (DECSET 1005) to `seq`.
///
/// Values are clamped to the range xterm can represent in this mode
/// (`char::MAX` fits in `u16`), then encoded as a UTF-8 code point: one byte for
/// values ≤ 127, two bytes above that.
pub(crate) fn push_mouse_utf8_field(seq: &mut Vec<u8>, value: u16) {
    let code_point = char::from_u32(value as u32).unwrap_or('\u{fffd}');
    let mut buf = [0u8; 4];
    seq.extend_from_slice(code_point.encode_utf8(&mut buf).as_bytes());
}
