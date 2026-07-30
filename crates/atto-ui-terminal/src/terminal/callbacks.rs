//! vt100 parser callback glue ([`TerminalCallbacks`] implementing
//! `vt100::Callbacks`), OSC sequence parsing helpers, paste encoding, and
//! clipboard-copy backends.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TerminalCallbackEvent {
    WindowTitle(String),
    WindowIconName(String),
    AudibleBell,
    ClipboardCopy(TerminalClipboardCopy),
    UnhandledOsc {
        params: Vec<Vec<u8>>,
        row: usize,
        col: u16,
    },
    CursorShape(TerminalCursorShape),
}

#[derive(Default)]
pub(crate) struct TerminalCallbacks {
    pub(crate) events: Vec<TerminalCallbackEvent>,
}

impl TerminalCallbacks {
    pub(crate) fn take_events(&mut self) -> Vec<TerminalCallbackEvent> {
        std::mem::take(&mut self.events)
    }
}

impl vt100::Callbacks for TerminalCallbacks {
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.events.push(TerminalCallbackEvent::AudibleBell);
    }

    fn set_window_icon_name(&mut self, _: &mut vt100::Screen, icon_name: &[u8]) {
        self.events.push(TerminalCallbackEvent::WindowIconName(
            string_from_terminal_bytes(icon_name),
        ));
    }

    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.events.push(TerminalCallbackEvent::WindowTitle(
            string_from_terminal_bytes(title),
        ));
    }

    fn copy_to_clipboard(&mut self, _: &mut vt100::Screen, selector: &[u8], data: &[u8]) {
        self.events.push(TerminalCallbackEvent::ClipboardCopy(
            TerminalClipboardCopy {
                selector: selector.to_vec(),
                data: data.to_vec(),
            },
        ));
    }

    fn unhandled_osc(&mut self, screen: &mut vt100::Screen, params: &[&[u8]]) {
        let (row, col) = current_absolute_position_for_screen(screen);
        self.events.push(TerminalCallbackEvent::UnhandledOsc {
            params: params.iter().map(|param| param.to_vec()).collect(),
            row,
            col,
        });
    }

    fn unhandled_csi(
        &mut self,
        _: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        if let Some(shape) = parse_decscusr_cursor_shape(i1, i2, params, c) {
            self.events.push(TerminalCallbackEvent::CursorShape(shape));
        }
    }
}

pub(crate) enum TerminalCallbackDispatch {
    WindowTitle(TextCallback, String),
    WindowIconName(TextCallback, String),
    AudibleBell(BellCallback),
    ClipboardCopy(ClipboardCopyCallback, TerminalClipboardCopy),
    CommandFinished(CommandFinishedCallback, TerminalCommandBlock),
    SystemClipboardCopy(String),
}

pub(crate) fn terminal_parser(rows: u16, cols: u16, scrollback_len: usize) -> TerminalParser {
    vt100::Parser::new_with_callbacks(rows, cols, scrollback_len, TerminalCallbacks::default())
}

pub(crate) fn current_absolute_position_for_screen(screen: &mut vt100::Screen) -> (usize, u16) {
    let current_scrollback = screen.scrollback();
    screen.set_scrollback(usize::MAX);
    let max_scrollback = screen.scrollback();
    screen.set_scrollback(current_scrollback);
    let (row, col) = screen.cursor_position();
    (max_scrollback.saturating_add(usize::from(row)), col)
}

pub(crate) fn string_from_terminal_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub(crate) fn parse_osc133_exit_code(bytes: &[u8]) -> Option<i32> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

pub(crate) fn parse_decscusr_cursor_shape(
    i1: Option<u8>,
    i2: Option<u8>,
    params: &[&[u16]],
    c: char,
) -> Option<TerminalCursorShape> {
    if i1 != Some(b' ') || i2.is_some() || c != 'q' {
        return None;
    }
    let style = params
        .first()
        .and_then(|param| param.first())
        .copied()
        .unwrap_or(0);
    match style {
        0..=2 => Some(TerminalCursorShape::Block),
        3 | 4 => Some(TerminalCursorShape::Underline),
        5 | 6 => Some(TerminalCursorShape::Bar),
        _ => None,
    }
}

pub(crate) fn parse_osc7_cwd(bytes: &[u8]) -> Option<String> {
    let uri = std::str::from_utf8(bytes).ok()?;
    let rest = uri.strip_prefix("file://")?;
    let path = if rest.starts_with('/') {
        rest
    } else {
        rest.get(rest.find('/')?..)?
    };
    if path.is_empty() {
        return None;
    }
    Some(percent_decode_uri_path(path))
}

pub(crate) fn percent_decode_uri_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            decoded.push((hi << 4) | lo);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

pub(crate) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn encode_paste_text(screen: &vt100::Screen, text: &str) -> Vec<u8> {
    if screen.bracketed_paste() {
        // Strip any embedded paste-mode markers from the payload before
        // wrapping. Without this, clipboard content containing `\x1b[201~`
        // would prematurely close paste mode and the remaining bytes would be
        // interpreted by the shell as typed input — a command-injection vector.
        // xterm handles this the same way (the terminator is removed, not
        // escaped).
        let sanitized = strip_bracketed_paste_markers(text.as_bytes());
        let mut buf = Vec::with_capacity(sanitized.len() + 16);
        buf.extend_from_slice(b"\x1b[200~");
        buf.extend_from_slice(&sanitized);
        buf.extend_from_slice(b"\x1b[201~");
        buf
    } else {
        text.as_bytes().to_vec()
    }
}

/// Removes any `\x1b[200~` / `\x1b[201~` bracketed-paste control sequences from
/// `bytes`, returning the cleaned payload.
pub(crate) fn strip_bracketed_paste_markers(bytes: &[u8]) -> Vec<u8> {
    const START: &[u8] = b"\x1b[200~";
    const END: &[u8] = b"\x1b[201~";
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(START) {
            i += START.len();
        } else if bytes[i..].starts_with(END) {
            i += END.len();
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

pub(crate) fn copy_text_with_arboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text.to_owned())?;
    Ok(())
}

pub(crate) fn copy_text_with_backends<O, A>(text: &str, osc52: O, arboard: A) -> Result<()>
where
    O: FnOnce(&str) -> Result<()>,
    A: FnOnce(&str) -> Result<()>,
{
    let osc52_result = osc52(text);
    let arboard_result = arboard(text);
    if osc52_result.is_ok() || arboard_result.is_ok() {
        return Ok(());
    }

    let osc52_error = osc52_result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| "unknown OSC 52 error".to_string());
    let arboard_error = arboard_result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| "unknown arboard error".to_string());
    Err(anyhow!(
        "failed to copy text to system clipboard via OSC 52 ({osc52_error}) or arboard ({arboard_error})"
    ))
}
