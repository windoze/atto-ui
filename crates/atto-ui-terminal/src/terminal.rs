use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow, bail, ensure};
use base64::Engine;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, ExitStatus, PtySize, native_pty_system};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use atto_ui::composable::{
    Capture, ComponentAction, ComponentContext, EventOutcome, EventResult, MouseCoordinateSpace,
    ScrollConfig,
};
use atto_ui::theme::Theme;

use crate::selection::{
    TerminalSelectionPosition, TerminalSelectionRange, TerminalSelectionState,
    position_for_view_cell, selected_cell_ranges_for_screen_row, selected_text_from_screen,
    visible_top_row,
};

const DEFAULT_SCROLLBACK_LEN: usize = 2000;
const DEFAULT_SCROLL_STEP: u16 = 3;
const COMMAND_SEPARATOR_SYMBOL: &str = "─";
const COMMAND_FAILURE_SYMBOL: &str = "!";

/// Keyboard shortcut used to release terminal input capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalShortcut {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

/// Visual treatment for OSC 133 command blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalCommandBlockPresentation {
    #[default]
    Disabled,
    Enabled,
}

impl TerminalCommandBlockPresentation {
    pub const fn enabled() -> Self {
        Self::Enabled
    }

    const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

impl TerminalShortcut {
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    fn matches(&self, event: KeyEvent) -> bool {
        if event.code != self.code {
            match (event.code, self.code) {
                (KeyCode::Char(a), KeyCode::Char(b)) if a.eq_ignore_ascii_case(&b) => {}
                _ => return false,
            }
        }
        if event.kind == KeyEventKind::Release {
            return false;
        }
        event.modifiers == self.modifiers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_shared() -> TerminalShared {
        TerminalShared {
            parser: terminal_parser(24, 80, DEFAULT_SCROLLBACK_LEN),
            scrollback_len: DEFAULT_SCROLLBACK_LEN,
            input: VecDeque::new(),
            on_input: None,
            input_forward: None,
            on_exit: None,
            on_window_title: None,
            on_window_icon_name: None,
            on_audible_bell: None,
            on_clipboard_copy: None,
            on_command_finished: None,
            system_clipboard: None,
            exit_status: None,
            process_running: false,
            window_title: None,
            window_icon_name: None,
            audible_bell_count: 0,
            last_clipboard_copy: None,
            last_system_clipboard_text: None,
            last_system_clipboard_error: None,
            capture: true,
            release_shortcut: default_release_shortcut(),
            prefix_shortcut: default_prefix_shortcut(),
            prefix_bindings: default_prefix_bindings(),
            prefix_pending: false,
            copy_mode: None,
            copy_buffer: None,
            selection: TerminalSelectionState::default(),
            command_marks: Vec::new(),
            current_cwd: None,
            dsr_tail: Vec::new(),
        }
    }

    fn mouse_at(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn mouse_coords_local_uses_explicit_coordinate_space() {
        let area = Some(Rect::new(10, 5, 4, 3));

        assert_eq!(
            mouse_coords_local(area, mouse_at(11, 6), MouseCoordinateSpace::Absolute),
            Some((1, 1))
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(1, 1), MouseCoordinateSpace::Absolute),
            None
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(1, 1), MouseCoordinateSpace::Local),
            Some((1, 1))
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(11, 6), MouseCoordinateSpace::Local),
            None
        );
    }

    #[test]
    fn dsr_responses_handle_split_packets() {
        let mut shared = test_shared();

        assert!(collect_dsr_responses(&mut shared, b"\x1b[?6").is_empty());
        let responses = collect_dsr_responses(&mut shared, b"n");
        assert_eq!(responses, vec![b"\x1b[?1;1R".to_vec()]);

        assert!(collect_dsr_responses(&mut shared, b"\x1b[?").is_empty());
        let responses = collect_dsr_responses(&mut shared, b"5n");
        assert_eq!(responses, vec![b"\x1b[?0n".to_vec()]);
    }

    #[test]
    fn dsr_complete_packets_do_not_repeat_on_later_output() {
        let mut shared = test_shared();

        let responses = collect_dsr_responses(&mut shared, b"\x1b[6n");
        assert_eq!(responses, vec![b"\x1b[1;1R".to_vec()]);

        assert!(collect_dsr_responses(&mut shared, b"x").is_empty());
        assert!(shared.dsr_tail.is_empty());
    }

    #[test]
    fn osc133_and_osc7_record_command_block_marks() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        handle.process_output_str(
            "\x1b]7;file://host/tmp/project%20one\x07\
             \x1b]133;A\x07$ echo ok\
             \x1b]133;B\x07\r\n\
             \x1b]133;C\x07ok\r\n\
             \x1b]133;D;7\x07",
        );

        let shared = terminal.shared.lock();
        assert_eq!(shared.current_cwd.as_deref(), Some("/tmp/project one"));
        assert_eq!(shared.command_marks.len(), 1);
        assert_eq!(
            shared.command_marks[0],
            TerminalCommandBlock {
                prompt_start: Some(0),
                command_start: Some(0),
                output_start: Some(1),
                end: Some(2),
                exit_code: Some(7),
                cwd: Some("/tmp/project one".to_string()),
            }
        );
    }

    #[test]
    fn osc133_missing_markers_degrade_to_partial_blocks() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        handle.process_output_str("plain output\r\n\x1b]133;D;0\x07");

        let shared = terminal.shared.lock();
        assert_eq!(shared.command_marks.len(), 1);
        assert_eq!(
            shared.command_marks[0],
            TerminalCommandBlock {
                prompt_start: Some(1),
                end: Some(1),
                exit_code: Some(0),
                ..TerminalCommandBlock::default()
            }
        );
    }

    #[test]
    fn osc7_decodes_file_uri_paths() {
        assert_eq!(
            parse_osc7_cwd(b"file://localhost/Users/test/project%20one").as_deref(),
            Some("/Users/test/project one")
        );
        assert_eq!(
            parse_osc7_cwd(b"file:///tmp/%E4%BD%A0%E5%A5%BD").as_deref(),
            Some("/tmp/你好")
        );
        assert_eq!(parse_osc7_cwd(b"https://example.invalid/tmp"), None);
    }

    #[test]
    fn system_clipboard_backend_uses_arboard_when_osc52_fails() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let osc52_calls = Arc::clone(&calls);
        let arboard_calls = Arc::clone(&calls);

        copy_text_with_backends(
            "copy me",
            move |text| {
                osc52_calls.lock().push(format!("osc52:{text}"));
                Err(anyhow!("osc52 failed"))
            },
            move |text| {
                arboard_calls.lock().push(format!("arboard:{text}"));
                Ok(())
            },
        )
        .expect("arboard fallback should succeed");

        assert_eq!(
            calls.lock().as_slice(),
            ["osc52:copy me", "arboard:copy me"]
        );
    }

    #[test]
    fn system_clipboard_backend_reports_when_both_paths_fail() {
        let error = copy_text_with_backends(
            "copy me",
            |_| Err(anyhow!("osc52 failed")),
            |_| Err(anyhow!("arboard failed")),
        )
        .expect_err("both backends should fail")
        .to_string();

        assert!(error.contains("osc52 failed"));
        assert!(error.contains("arboard failed"));
    }
}

fn default_release_shortcut() -> TerminalShortcut {
    TerminalShortcut {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    }
}

fn default_prefix_shortcut() -> TerminalShortcut {
    TerminalShortcut {
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::CONTROL,
    }
}

fn default_prefix_bindings() -> Vec<TerminalPrefixBinding> {
    vec![
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::F(10), KeyModifiers::NONE),
            TerminalPrefixCommand::ActivateMenu,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('w'), KeyModifiers::NONE),
            TerminalPrefixCommand::ToggleWindowManagement,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('z'), KeyModifiers::NONE),
            TerminalPrefixCommand::ToggleMaximize,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('['), KeyModifiers::NONE),
            TerminalPrefixCommand::EnterCopyMode,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char(']'), KeyModifiers::NONE),
            TerminalPrefixCommand::PasteCopyBuffer,
        ),
    ]
}

fn prefix_shortcut_from_letter(letter: char) -> Result<TerminalShortcut> {
    normalize_prefix_shortcut(TerminalShortcut::new(
        KeyCode::Char(letter),
        KeyModifiers::CONTROL,
    ))
}

fn normalize_prefix_shortcut(shortcut: TerminalShortcut) -> Result<TerminalShortcut> {
    ensure!(
        shortcut.modifiers == KeyModifiers::CONTROL,
        "terminal prefix shortcut must be plain Ctrl+<ASCII letter>"
    );
    let KeyCode::Char(letter) = shortcut.code else {
        bail!("terminal prefix shortcut must be plain Ctrl+<ASCII letter>");
    };
    ensure!(
        letter.is_ascii_alphabetic(),
        "terminal prefix shortcut must be plain Ctrl+<ASCII letter>"
    );
    Ok(TerminalShortcut {
        code: KeyCode::Char(letter.to_ascii_lowercase()),
        modifiers: KeyModifiers::CONTROL,
    })
}

fn normalize_prefix_binding_shortcut(shortcut: TerminalShortcut) -> TerminalShortcut {
    let code = match shortcut.code {
        KeyCode::Char(letter) => KeyCode::Char(letter.to_ascii_lowercase()),
        code => code,
    };
    TerminalShortcut {
        code,
        modifiers: shortcut.modifiers,
    }
}

type InputCallback = Arc<dyn Fn(&[u8]) + Send + Sync>;
type ExitCallback = Arc<dyn Fn(ExitStatus) + Send + Sync>;
type TextCallback = Arc<dyn Fn(&str) + Send + Sync>;
type BellCallback = Arc<dyn Fn() + Send + Sync>;
type ClipboardCopyCallback = Arc<dyn Fn(&TerminalClipboardCopy) + Send + Sync>;
type CommandFinishedCallback = Arc<dyn Fn(&TerminalCommandBlock) + Send + Sync>;
type SystemClipboard = Arc<dyn TerminalSystemClipboard>;
type TerminalParser = vt100::Parser<TerminalCallbacks>;

/// System clipboard sink used by [`TerminalEmulator`] copy operations.
///
/// The default implementation sends an OSC 52 clipboard request to the host terminal first and
/// then tries `arboard`, so remote-capable terminal clipboard support takes priority while native
/// clipboard APIs still cover hosts that ignore OSC 52.
pub trait TerminalSystemClipboard: Send + Sync {
    fn copy_text(&self, text: &str) -> Result<()>;
}

impl<F> TerminalSystemClipboard for F
where
    F: Fn(&str) -> Result<()> + Send + Sync,
{
    fn copy_text(&self, text: &str) -> Result<()> {
        self(text)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DefaultTerminalSystemClipboard;

impl TerminalSystemClipboard for DefaultTerminalSystemClipboard {
    fn copy_text(&self, text: &str) -> Result<()> {
        copy_text_with_backends(
            text,
            |text| atto_ui::clipboard::copy_to_system_clipboard(text).map_err(Into::into),
            copy_text_with_arboard,
        )
    }
}

/// OSC 52 clipboard-copy request observed in the terminal output stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalClipboardCopy {
    /// Clipboard selector from the OSC 52 sequence, for example `c`.
    pub selector: Vec<u8>,
    /// Base64-encoded clipboard payload from the OSC 52 sequence.
    pub data: Vec<u8>,
}

impl TerminalClipboardCopy {
    /// Returns whether this OSC 52 request targets the standard clipboard selection.
    pub fn targets_system_clipboard(&self) -> bool {
        self.selector.is_empty() || self.selector.contains(&b'c')
    }

    /// Decodes the OSC 52 base64 payload as UTF-8 clipboard text.
    pub fn decoded_text(&self) -> Result<String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.data)
            .map_err(|error| anyhow!("invalid OSC 52 clipboard payload: {error}"))?;
        String::from_utf8(bytes)
            .map_err(|error| anyhow!("OSC 52 clipboard payload is not UTF-8 text: {error}"))
    }
}

/// Command selected by the terminal prefix key table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPrefixCommand {
    ActivateMenu,
    ToggleWindowManagement,
    ToggleMaximize,
    EnterCopyMode,
    PasteCopyBuffer,
    SendPrefix,
}

/// One configurable binding in the prefix command table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalPrefixBinding {
    pub shortcut: TerminalShortcut,
    pub command: TerminalPrefixCommand,
}

impl TerminalPrefixBinding {
    pub fn new(shortcut: TerminalShortcut, command: TerminalPrefixCommand) -> Self {
        Self {
            shortcut: normalize_prefix_binding_shortcut(shortcut),
            command,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalCopyModeState {
    cursor: TerminalSelectionPosition,
    selecting: bool,
}

impl TerminalCopyModeState {
    fn new(cursor: TerminalSelectionPosition) -> Self {
        Self {
            cursor,
            selecting: false,
        }
    }
}

/// OSC 133/7 command block markers observed in terminal output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalCommandBlock {
    /// Absolute terminal row where the prompt started.
    pub prompt_start: Option<usize>,
    /// Absolute terminal row where the command text started.
    pub command_start: Option<usize>,
    /// Absolute terminal row where command output started.
    pub output_start: Option<usize>,
    /// Absolute terminal row where the command finished.
    pub end: Option<usize>,
    /// Command-level exit code reported by OSC 133 `D`, if present.
    pub exit_code: Option<i32>,
    /// Current working directory reported by OSC 7 for this block.
    pub cwd: Option<String>,
}

impl TerminalCommandBlock {
    fn at_prompt(row: usize, cwd: Option<String>) -> Self {
        Self {
            prompt_start: Some(row),
            cwd,
            ..Self::default()
        }
    }

    fn is_open(&self) -> bool {
        self.end.is_none()
    }

    fn has_command_activity(&self) -> bool {
        self.command_start.is_some() || self.output_start.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalCallbackEvent {
    WindowTitle(String),
    WindowIconName(String),
    AudibleBell,
    ClipboardCopy(TerminalClipboardCopy),
    UnhandledOsc { params: Vec<Vec<u8>>, row: usize },
}

#[derive(Default)]
struct TerminalCallbacks {
    events: Vec<TerminalCallbackEvent>,
}

impl TerminalCallbacks {
    fn take_events(&mut self) -> Vec<TerminalCallbackEvent> {
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
        self.events.push(TerminalCallbackEvent::UnhandledOsc {
            params: params.iter().map(|param| param.to_vec()).collect(),
            row: current_absolute_row_for_screen(screen),
        });
    }
}

enum TerminalCallbackDispatch {
    WindowTitle(TextCallback, String),
    WindowIconName(TextCallback, String),
    AudibleBell(BellCallback),
    ClipboardCopy(ClipboardCopyCallback, TerminalClipboardCopy),
    CommandFinished(CommandFinishedCallback, TerminalCommandBlock),
    SystemClipboardCopy(String),
}

fn terminal_parser(rows: u16, cols: u16, scrollback_len: usize) -> TerminalParser {
    vt100::Parser::new_with_callbacks(rows, cols, scrollback_len, TerminalCallbacks::default())
}

fn current_absolute_row_for_screen(screen: &mut vt100::Screen) -> usize {
    let current_scrollback = screen.scrollback();
    screen.set_scrollback(usize::MAX);
    let max_scrollback = screen.scrollback();
    screen.set_scrollback(current_scrollback);
    let row = screen.cursor_position().0;
    max_scrollback.saturating_add(usize::from(row))
}

fn string_from_terminal_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn parse_osc133_exit_code(bytes: &[u8]) -> Option<i32> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn parse_osc7_cwd(bytes: &[u8]) -> Option<String> {
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

fn percent_decode_uri_path(path: &str) -> String {
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

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode_paste_text(screen: &vt100::Screen, text: &str) -> Vec<u8> {
    if screen.bracketed_paste() {
        let mut buf = Vec::with_capacity(text.len() + 16);
        buf.extend_from_slice(b"\x1b[200~");
        buf.extend_from_slice(text.as_bytes());
        buf.extend_from_slice(b"\x1b[201~");
        buf
    } else {
        text.as_bytes().to_vec()
    }
}

fn copy_text_with_arboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text.to_owned())?;
    Ok(())
}

fn copy_text_with_backends<O, A>(text: &str, osc52: O, arboard: A) -> Result<()>
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CommandRowPresentation {
    separator: bool,
    output: bool,
    failed_marker: bool,
}

fn command_row_presentation(blocks: &[TerminalCommandBlock], row: usize) -> CommandRowPresentation {
    let mut presentation = CommandRowPresentation::default();
    for block in blocks {
        if block.prompt_start == Some(row) {
            presentation.separator = true;
        }

        if let Some(output_start) = block.output_start {
            let output_end = block.end.unwrap_or(usize::MAX);
            if row >= output_start && row < output_end {
                presentation.output = true;
            }
        }

        if block.exit_code.is_some_and(|code| code != 0)
            && block
                .end
                .or(block.output_start)
                .or(block.command_start)
                .or(block.prompt_start)
                == Some(row)
        {
            presentation.failed_marker = true;
        }
    }
    presentation
}

fn command_separator_start(screen: &vt100::Screen, row: u16, width: u16) -> u16 {
    let mut content_end = 0;
    for x in 0..width {
        let Some(cell) = screen.cell(row, x) else {
            continue;
        };
        if cell.is_wide_continuation() || cell.contents().is_empty() {
            continue;
        }
        content_end = x.saturating_add(1);
    }
    if content_end == 0 {
        0
    } else {
        content_end.saturating_add(1).min(width)
    }
}

fn command_output_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-output")
        .unwrap_or_else(|| {
            let mut style = Style::default();
            if let Some(bg) = theme.status_bar.bg {
                style = style.bg(bg);
            }
            style
        })
}

fn command_separator_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-separator")
        .unwrap_or_else(|| theme.status_bar_key.add_modifier(Modifier::BOLD))
}

fn command_failure_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-failure")
        .or_else(|| theme.named_style("status-segment-error"))
        .unwrap_or_else(|| Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
}

struct TerminalShared {
    parser: TerminalParser,
    scrollback_len: usize,
    input: VecDeque<u8>,
    on_input: Option<InputCallback>,
    input_forward: Option<InputCallback>,
    on_exit: Option<ExitCallback>,
    on_window_title: Option<TextCallback>,
    on_window_icon_name: Option<TextCallback>,
    on_audible_bell: Option<BellCallback>,
    on_clipboard_copy: Option<ClipboardCopyCallback>,
    on_command_finished: Option<CommandFinishedCallback>,
    system_clipboard: Option<SystemClipboard>,
    exit_status: Option<ExitStatus>,
    process_running: bool,
    window_title: Option<String>,
    window_icon_name: Option<String>,
    audible_bell_count: u64,
    last_clipboard_copy: Option<TerminalClipboardCopy>,
    last_system_clipboard_text: Option<String>,
    last_system_clipboard_error: Option<String>,
    capture: bool,
    release_shortcut: TerminalShortcut,
    prefix_shortcut: TerminalShortcut,
    prefix_bindings: Vec<TerminalPrefixBinding>,
    prefix_pending: bool,
    copy_mode: Option<TerminalCopyModeState>,
    copy_buffer: Option<String>,
    selection: TerminalSelectionState,
    command_marks: Vec<TerminalCommandBlock>,
    current_cwd: Option<String>,
    dsr_tail: Vec<u8>,
}

impl TerminalShared {
    fn set_capture(&mut self, capture: bool) {
        self.capture = capture;
        if !capture {
            self.prefix_pending = false;
            self.copy_mode = None;
        }
    }

    fn set_prefix_shortcut(&mut self, shortcut: TerminalShortcut) {
        self.prefix_shortcut = shortcut;
        self.prefix_pending = false;
    }

    fn set_prefix_binding(&mut self, binding: TerminalPrefixBinding) {
        if let Some(existing) = self
            .prefix_bindings
            .iter_mut()
            .find(|existing| existing.shortcut == binding.shortcut)
        {
            *existing = binding;
        } else {
            self.prefix_bindings.push(binding);
        }
        self.prefix_pending = false;
    }

    fn set_prefix_bindings(&mut self, bindings: impl IntoIterator<Item = TerminalPrefixBinding>) {
        self.prefix_bindings.clear();
        for binding in bindings {
            self.set_prefix_binding(binding);
        }
        self.prefix_pending = false;
    }

    fn prefix_command_for_event(&self, event: KeyEvent) -> Option<TerminalPrefixCommand> {
        if self.prefix_shortcut.matches(event) {
            return Some(TerminalPrefixCommand::SendPrefix);
        }
        if event.kind == KeyEventKind::Release {
            return None;
        }
        self.prefix_bindings
            .iter()
            .find(|binding| binding.shortcut.matches(event))
            .map(|binding| binding.command)
    }

    fn apply_callback_events(
        &mut self,
        events: Vec<TerminalCallbackEvent>,
    ) -> Vec<TerminalCallbackDispatch> {
        let mut dispatches = Vec::new();
        for event in events {
            match event {
                TerminalCallbackEvent::WindowTitle(title) => {
                    self.window_title = Some(title.clone());
                    if let Some(callback) = self.on_window_title.clone() {
                        dispatches.push(TerminalCallbackDispatch::WindowTitle(callback, title));
                    }
                }
                TerminalCallbackEvent::WindowIconName(icon_name) => {
                    self.window_icon_name = Some(icon_name.clone());
                    if let Some(callback) = self.on_window_icon_name.clone() {
                        dispatches.push(TerminalCallbackDispatch::WindowIconName(
                            callback, icon_name,
                        ));
                    }
                }
                TerminalCallbackEvent::AudibleBell => {
                    self.audible_bell_count = self.audible_bell_count.saturating_add(1);
                    if let Some(callback) = self.on_audible_bell.clone() {
                        dispatches.push(TerminalCallbackDispatch::AudibleBell(callback));
                    }
                }
                TerminalCallbackEvent::ClipboardCopy(copy) => {
                    self.last_clipboard_copy = Some(copy.clone());
                    if copy.targets_system_clipboard()
                        && let Ok(text) = copy.decoded_text()
                    {
                        self.copy_buffer = Some(text.clone());
                        dispatches.push(TerminalCallbackDispatch::SystemClipboardCopy(text));
                    }
                    if let Some(callback) = self.on_clipboard_copy.clone() {
                        dispatches.push(TerminalCallbackDispatch::ClipboardCopy(callback, copy));
                    }
                }
                TerminalCallbackEvent::UnhandledOsc { params, row } => {
                    if let Some(block) = self.apply_unhandled_osc(&params, row)
                        && let Some(callback) = self.on_command_finished.clone()
                    {
                        dispatches.push(TerminalCallbackDispatch::CommandFinished(callback, block));
                    }
                }
            }
        }
        dispatches
    }

    fn apply_unhandled_osc(
        &mut self,
        params: &[Vec<u8>],
        row: usize,
    ) -> Option<TerminalCommandBlock> {
        match params {
            [kind, rest @ ..] if kind.as_slice() == b"133" => self.apply_osc133_marker(rest, row),
            [kind, cwd] if kind.as_slice() == b"7" => {
                if let Some(cwd) = parse_osc7_cwd(cwd) {
                    self.current_cwd = Some(cwd.clone());
                    if let Some(block) = self.current_command_block_mut() {
                        block.cwd = Some(cwd);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn apply_osc133_marker(
        &mut self,
        params: &[Vec<u8>],
        row: usize,
    ) -> Option<TerminalCommandBlock> {
        let marker = params.first().and_then(|marker| marker.first()).copied()?;
        match marker {
            b'A' => {
                self.record_prompt_start(row);
                None
            }
            b'B' => {
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, cwd);
                block.command_start = Some(row);
                None
            }
            b'C' => {
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, cwd);
                block.output_start = Some(row);
                None
            }
            b'D' => {
                let exit_code = params.get(1).and_then(|code| parse_osc133_exit_code(code));
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, cwd);
                block.end = Some(row);
                block.exit_code = exit_code;
                Some(block.clone())
            }
            _ => None,
        }
    }

    fn record_prompt_start(&mut self, row: usize) {
        let cwd = self.current_cwd.clone();
        match self.command_marks.last_mut() {
            Some(block) if block.is_open() && !block.has_command_activity() => {
                block.prompt_start = Some(row);
                block.cwd = cwd;
            }
            _ => self
                .command_marks
                .push(TerminalCommandBlock::at_prompt(row, cwd)),
        }
    }

    fn open_command_block(&mut self, row: usize, cwd: Option<String>) -> &mut TerminalCommandBlock {
        let needs_new_block = self
            .command_marks
            .last()
            .is_none_or(|block| !block.is_open());
        if needs_new_block {
            self.command_marks.push(TerminalCommandBlock {
                prompt_start: Some(row),
                cwd,
                ..TerminalCommandBlock::default()
            });
        }
        self.command_marks.last_mut().expect("command block exists")
    }

    fn current_command_block_mut(&mut self) -> Option<&mut TerminalCommandBlock> {
        self.command_marks
            .last_mut()
            .filter(|block| block.is_open())
    }

    fn queue_input(&mut self, bytes: &[u8]) {
        self.input.extend(bytes);
    }

    fn max_scrollback(&mut self) -> usize {
        let screen = self.parser.screen_mut();
        let current = screen.scrollback();
        screen.set_scrollback(usize::MAX);
        let max = screen.scrollback();
        screen.set_scrollback(current);
        max
    }

    fn scrollback_offset(&self) -> usize {
        self.parser.screen().scrollback()
    }

    fn set_scrollback_offset(&mut self, offset: usize) {
        self.parser.screen_mut().set_scrollback(offset);
    }

    fn set_scrollback_from_scroll_offset(&mut self, scroll_offset: u16) {
        let max = self.max_scrollback().min(u16::MAX as usize);
        let y = scroll_offset.min(max as u16) as usize;
        let offset = max.saturating_sub(y);
        self.set_scrollback_offset(offset);
    }

    fn enter_copy_mode(&mut self) {
        let cursor = self.current_copy_mode_position();
        self.copy_mode = Some(TerminalCopyModeState::new(cursor));
        self.prefix_pending = false;
        self.selection.clear();
        self.ensure_copy_mode_cursor_visible();
    }

    fn cancel_copy_mode(&mut self) {
        self.copy_mode = None;
        self.selection.clear();
    }

    fn finish_copy_mode_copy(&mut self) -> Option<String> {
        let text = self.copy_selection();
        self.copy_mode = None;
        self.selection.clear();
        text
    }

    fn copy_selection(&mut self) -> Option<String> {
        let text = self.selected_text()?;
        self.copy_buffer = Some(text.clone());
        Some(text)
    }

    fn paste_copy_buffer_bytes(&self) -> Option<Vec<u8>> {
        self.copy_buffer
            .as_deref()
            .map(|text| encode_paste_text(self.parser.screen(), text))
    }

    fn selected_text(&mut self) -> Option<String> {
        let range = self.selection.range()?;
        let max_scrollback = self.max_scrollback();
        selected_text_from_screen(self.parser.screen_mut(), max_scrollback, range)
    }

    fn current_copy_mode_position(&mut self) -> TerminalSelectionPosition {
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let (row, col) = screen.cursor_position();
        position_for_view_cell(
            max_scrollback,
            screen.scrollback(),
            rows,
            cols,
            row.min(rows.saturating_sub(1)),
            col.min(cols),
        )
    }

    fn begin_copy_mode_selection(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        self.selection.start_keyboard(cursor);
        if let Some(mode) = &mut self.copy_mode {
            mode.selecting = true;
        }
    }

    fn move_copy_mode_cursor(&mut self, row_delta: isize, col_delta: isize) -> bool {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return false;
        };
        let row = if row_delta.is_negative() {
            cursor.row.saturating_sub(row_delta.unsigned_abs())
        } else {
            cursor.row.saturating_add(row_delta as usize)
        };
        let col = if col_delta.is_negative() {
            cursor
                .col
                .saturating_sub(col_delta.unsigned_abs().min(u16::MAX as usize) as u16)
        } else {
            cursor
                .col
                .saturating_add((col_delta as usize).min(u16::MAX as usize) as u16)
        };
        self.set_copy_mode_cursor(TerminalSelectionPosition::new(row, col))
    }

    fn set_copy_mode_cursor(&mut self, position: TerminalSelectionPosition) -> bool {
        let position = self.clamp_copy_mode_position(position);
        let Some(mode) = &mut self.copy_mode else {
            return false;
        };
        if mode.cursor == position {
            return false;
        }
        mode.cursor = position;
        if mode.selecting {
            self.selection.update(position);
        }
        self.ensure_copy_mode_cursor_visible();
        true
    }

    fn move_copy_mode_cursor_to_column(&mut self, col: u16) -> bool {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return false;
        };
        self.set_copy_mode_cursor(TerminalSelectionPosition::new(cursor.row, col))
    }

    fn move_copy_mode_cursor_by_page(&mut self, page_delta: isize) -> bool {
        let rows = self.parser.screen().size().0.max(1) as isize;
        self.move_copy_mode_cursor(page_delta.saturating_mul(rows), 0)
    }

    fn clamp_copy_mode_position(
        &mut self,
        position: TerminalSelectionPosition,
    ) -> TerminalSelectionPosition {
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let last_row = max_scrollback.saturating_add(usize::from(rows.saturating_sub(1)));
        TerminalSelectionPosition::new(position.row.min(last_row), position.col.min(cols))
    }

    fn ensure_copy_mode_cursor_visible(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, _) = screen.size();
        if rows == 0 {
            return;
        }
        let current = screen.scrollback();
        let top = visible_top_row(max_scrollback, current);
        let height = usize::from(rows);
        let bottom = top.saturating_add(height.saturating_sub(1));
        let desired = if cursor.row < top {
            max_scrollback.saturating_sub(cursor.row)
        } else if cursor.row > bottom {
            max_scrollback.saturating_sub(cursor.row.saturating_sub(height.saturating_sub(1)))
        } else {
            current
        };
        if desired != current {
            self.parser
                .screen_mut()
                .set_scrollback(desired.min(max_scrollback));
        }
    }

    fn scroll_copy_mode_view(&mut self, line_delta: isize) -> bool {
        if self.copy_mode.is_none() {
            return false;
        }
        let max = self.max_scrollback();
        let current = self.parser.screen().scrollback();
        let desired = if line_delta.is_negative() {
            current.saturating_sub(line_delta.unsigned_abs())
        } else {
            current.saturating_add(line_delta as usize).min(max)
        };
        if desired != current {
            self.parser.screen_mut().set_scrollback(desired);
            self.clamp_copy_mode_cursor_to_visible();
            return true;
        }
        false
    }

    fn clamp_copy_mode_cursor_to_visible(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, _) = screen.size();
        if rows == 0 {
            return;
        }
        let top = visible_top_row(max_scrollback, screen.scrollback());
        let bottom = top.saturating_add(usize::from(rows.saturating_sub(1)));
        let row = cursor.row.clamp(top, bottom);
        if row != cursor.row {
            let _ = self.set_copy_mode_cursor(TerminalSelectionPosition::new(row, cursor.col));
        }
    }
}

enum CapturedKeyAction {
    Consumed,
    Dispatch(Vec<u8>),
    Component(ComponentAction),
    SystemClipboardCopy(String),
}

fn handle_captured_key(shared: &mut TerminalShared, event: KeyEvent) -> CapturedKeyAction {
    if shared.copy_mode.is_some() {
        return handle_copy_mode_key(shared, event);
    }
    if shared.release_shortcut.matches(event) {
        shared.set_capture(false);
        return CapturedKeyAction::Consumed;
    }
    if event.kind == KeyEventKind::Release {
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

fn handle_copy_mode_key(shared: &mut TerminalShared, event: KeyEvent) -> CapturedKeyAction {
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

fn handle_prefix_command(
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

fn encode_prefix_literal(shared: &TerminalShared) -> Option<Vec<u8>> {
    encode_key_event(
        shared.parser.screen(),
        KeyEvent::new(
            shared.prefix_shortcut.code,
            shared.prefix_shortcut.modifiers,
        ),
    )
}

fn encode_prefix_fallback(shared: &TerminalShared, event: KeyEvent) -> Option<Vec<u8>> {
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

type PtyChild = Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>;

struct TerminalProcess {
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: PtyChild,
    reader_alive: Arc<AtomicBool>,
    reader_thread: Option<thread::JoinHandle<()>>,
    exit_watcher_thread: Option<thread::JoinHandle<()>>,
    last_size: (u16, u16),
}

impl TerminalProcess {
    fn resize_if_needed(&mut self, rows: u16, cols: u16) {
        if self.last_size == (rows, cols) {
            return;
        }
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.last_size = (rows, cols);
    }

    fn record_exit_if_ready(&mut self, shared: &Arc<Mutex<TerminalShared>>) -> bool {
        try_record_child_exit(shared, &self.child)
    }

    fn shutdown(&mut self, shared: &Arc<Mutex<TerminalShared>>) {
        let already_exited = self.record_exit_if_ready(shared);
        self.reader_alive.store(false, Ordering::Relaxed);
        if !already_exited {
            let _ = self.child.lock().kill();
        }
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.exit_watcher_thread.take() {
            let _ = handle.join();
        }
    }
}

fn try_record_child_exit(shared: &Arc<Mutex<TerminalShared>>, child: &PtyChild) -> bool {
    let status = match child.lock().try_wait() {
        Ok(Some(status)) => status,
        Ok(None) | Err(_) => return false,
    };
    record_exit_status(shared, status);
    true
}

fn record_exit_status(shared: &Arc<Mutex<TerminalShared>>, status: ExitStatus) {
    let callback = {
        let mut shared = shared.lock();
        if shared.exit_status.is_some() {
            return;
        }
        shared.exit_status = Some(status.clone());
        shared.process_running = false;
        shared.input_forward = None;
        shared.on_exit.clone()
    };

    if let Some(callback) = callback {
        callback(status);
    }
}

fn dispatch_input(shared: &Arc<Mutex<TerminalShared>>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let callbacks = {
        let mut shared = shared.lock();
        shared.queue_input(bytes);
        let mut callbacks = Vec::new();
        if let Some(cb) = shared.on_input.clone() {
            callbacks.push(cb);
        }
        if let Some(cb) = shared.input_forward.clone() {
            callbacks.push(cb);
        }
        callbacks
    };
    for cb in callbacks {
        cb(bytes);
    }
}

fn forward_input(shared: &Arc<Mutex<TerminalShared>>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let cb = { shared.lock().input_forward.clone() };
    if let Some(cb) = cb {
        cb(bytes);
    }
}

fn dispatch_system_clipboard_copy(shared: &Arc<Mutex<TerminalShared>>, text: &str) {
    let clipboard = { shared.lock().system_clipboard.clone() };
    let Some(clipboard) = clipboard else {
        return;
    };
    let result = clipboard.copy_text(text);
    let mut shared = shared.lock();
    shared.last_system_clipboard_text = Some(text.to_string());
    shared.last_system_clipboard_error = result.err().map(|error| error.to_string());
}

fn dispatch_terminal_callback_events(
    shared: &Arc<Mutex<TerminalShared>>,
    dispatches: Vec<TerminalCallbackDispatch>,
) {
    for dispatch in dispatches {
        match dispatch {
            TerminalCallbackDispatch::WindowTitle(callback, title) => callback(&title),
            TerminalCallbackDispatch::WindowIconName(callback, icon_name) => callback(&icon_name),
            TerminalCallbackDispatch::AudibleBell(callback) => callback(),
            TerminalCallbackDispatch::ClipboardCopy(callback, copy) => callback(&copy),
            TerminalCallbackDispatch::CommandFinished(callback, block) => callback(&block),
            TerminalCallbackDispatch::SystemClipboardCopy(text) => {
                dispatch_system_clipboard_copy(shared, &text);
            }
        }
    }
}

enum DsrResponse {
    Cursor { private: bool },
    Status { private: bool },
}

fn collect_dsr_responses(shared: &mut TerminalShared, bytes: &[u8]) -> Vec<Vec<u8>> {
    if bytes.is_empty() {
        return Vec::new();
    }

    let mut combined = Vec::with_capacity(shared.dsr_tail.len() + bytes.len());
    combined.extend_from_slice(&shared.dsr_tail);
    combined.extend_from_slice(bytes);

    let mut responses = Vec::new();
    let mut idx = 0;
    let mut tail_start = combined.len();
    while idx < combined.len() {
        if combined[idx] != 0x1b {
            idx += 1;
            continue;
        }
        if idx + 1 >= combined.len() {
            tail_start = idx;
            break;
        }
        if combined[idx + 1] != b'[' {
            idx += 1;
            continue;
        }
        if idx + 2 >= combined.len() {
            tail_start = idx;
            break;
        }

        match combined[idx + 2] {
            b'6' | b'5' => {
                if idx + 3 >= combined.len() {
                    tail_start = idx;
                    break;
                }
                if combined[idx + 3] == b'n' {
                    responses.push(match combined[idx + 2] {
                        b'6' => DsrResponse::Cursor { private: false },
                        _ => DsrResponse::Status { private: false },
                    });
                    idx += 4;
                    continue;
                }
            }
            b'?' => {
                if idx + 3 >= combined.len() {
                    tail_start = idx;
                    break;
                }
                if matches!(combined[idx + 3], b'6' | b'5') {
                    if idx + 4 >= combined.len() {
                        tail_start = idx;
                        break;
                    }
                    if combined[idx + 4] == b'n' {
                        responses.push(match combined[idx + 3] {
                            b'6' => DsrResponse::Cursor { private: true },
                            _ => DsrResponse::Status { private: true },
                        });
                        idx += 5;
                        continue;
                    }
                }
            }
            _ => {}
        }
        idx += 1;
    }

    shared.dsr_tail.clear();
    shared.dsr_tail.extend_from_slice(&combined[tail_start..]);

    if responses.is_empty() {
        return Vec::new();
    }

    responses
        .into_iter()
        .map(|response| match response {
            DsrResponse::Cursor { private } => {
                let (row, col) = shared.parser.screen().cursor_position();
                let row = row.saturating_add(1);
                let col = col.saturating_add(1);
                if private {
                    format!("\x1b[?{row};{col}R").into_bytes()
                } else {
                    format!("\x1b[{row};{col}R").into_bytes()
                }
            }
            DsrResponse::Status { private } => {
                if private {
                    b"\x1b[?0n".to_vec()
                } else {
                    b"\x1b[0n".to_vec()
                }
            }
        })
        .collect()
}

/// A terminal emulator widget.
pub struct TerminalEmulator {
    shared: Arc<Mutex<TerminalShared>>,
    last_area: Option<Rect>,
    scroll_step: u16,
    capture_on_click: bool,
    command_block_presentation: TerminalCommandBlockPresentation,
    process: Option<TerminalProcess>,
    on_close: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl TerminalEmulator {
    pub fn new() -> Self {
        let parser = terminal_parser(24, 80, DEFAULT_SCROLLBACK_LEN);
        let shared = TerminalShared {
            parser,
            scrollback_len: DEFAULT_SCROLLBACK_LEN,
            input: VecDeque::new(),
            on_input: None,
            input_forward: None,
            on_exit: None,
            on_window_title: None,
            on_window_icon_name: None,
            on_audible_bell: None,
            on_clipboard_copy: None,
            on_command_finished: None,
            system_clipboard: Some(Arc::new(DefaultTerminalSystemClipboard)),
            exit_status: None,
            process_running: false,
            window_title: None,
            window_icon_name: None,
            audible_bell_count: 0,
            last_clipboard_copy: None,
            last_system_clipboard_text: None,
            last_system_clipboard_error: None,
            capture: true,
            release_shortcut: default_release_shortcut(),
            prefix_shortcut: default_prefix_shortcut(),
            prefix_bindings: default_prefix_bindings(),
            prefix_pending: false,
            copy_mode: None,
            copy_buffer: None,
            selection: TerminalSelectionState::default(),
            command_marks: Vec::new(),
            current_cwd: None,
            dsr_tail: Vec::with_capacity(4),
        };

        Self {
            shared: Arc::new(Mutex::new(shared)),
            last_area: None,
            scroll_step: DEFAULT_SCROLL_STEP,
            capture_on_click: true,
            command_block_presentation: TerminalCommandBlockPresentation::default(),
            process: None,
            on_close: None,
        }
    }

    pub fn handle(&self) -> TerminalHandle {
        TerminalHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn scrollback_len(self, len: usize) -> Self {
        {
            let mut shared = self.shared.lock();
            shared.scrollback_len = len;
            let (rows, cols) = shared.parser.screen().size();
            shared.parser = terminal_parser(rows, cols, len);
        }
        self
    }

    pub fn capture(self, capture: bool) -> Self {
        self.shared.lock().set_capture(capture);
        self
    }

    pub fn release_shortcut(self, shortcut: TerminalShortcut) -> Self {
        self.shared.lock().release_shortcut = shortcut;
        self
    }

    /// Sets the terminal prefix shortcut. Only plain `Ctrl+<ASCII letter>` is accepted.
    pub fn prefix_shortcut(self, shortcut: TerminalShortcut) -> Result<Self> {
        let shortcut = normalize_prefix_shortcut(shortcut)?;
        self.shared.lock().set_prefix_shortcut(shortcut);
        Ok(self)
    }

    /// Sets the terminal prefix key letter, using `Ctrl+letter` as the actual shortcut.
    pub fn prefix_key(self, letter: char) -> Result<Self> {
        self.prefix_shortcut(prefix_shortcut_from_letter(letter)?)
    }

    /// Adds or replaces one prefix command binding.
    pub fn prefix_binding(
        self,
        shortcut: TerminalShortcut,
        command: TerminalPrefixCommand,
    ) -> Self {
        self.shared
            .lock()
            .set_prefix_binding(TerminalPrefixBinding::new(shortcut, command));
        self
    }

    /// Replaces the prefix command table.
    pub fn prefix_bindings(
        self,
        bindings: impl IntoIterator<Item = TerminalPrefixBinding>,
    ) -> Self {
        self.shared.lock().set_prefix_bindings(bindings);
        self
    }

    pub fn scroll_step(mut self, step: u16) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn capture_on_click(mut self, enabled: bool) -> Self {
        self.capture_on_click = enabled;
        self
    }

    /// Enables or disables visual presentation for OSC 133 command blocks.
    pub fn command_block_presentation(
        mut self,
        presentation: TerminalCommandBlockPresentation,
    ) -> Self {
        self.command_block_presentation = presentation;
        self
    }

    /// Replaces the system clipboard backend used by selection, copy-mode, and OSC 52 copies.
    pub fn system_clipboard<C>(self, clipboard: C) -> Self
    where
        C: TerminalSystemClipboard + 'static,
    {
        self.shared.lock().system_clipboard = Some(Arc::new(clipboard));
        self
    }

    /// Disables system clipboard writes while preserving the terminal-local copy buffer.
    pub fn without_system_clipboard(self) -> Self {
        self.shared.lock().system_clipboard = None;
        self
    }

    pub fn on_input<F>(self, callback: F) -> Self
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        self.shared.lock().on_input = Some(Arc::new(callback));
        self
    }

    pub fn on_close<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires once when the attached subprocess exits.
    pub fn on_exit<F>(self, callback: F) -> Self
    where
        F: Fn(ExitStatus) + Send + Sync + 'static,
    {
        self.shared.lock().on_exit = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 0/2 updates the window title.
    pub fn on_window_title<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.shared.lock().on_window_title = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 0/1 updates the window icon name.
    pub fn on_window_icon_name<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.shared.lock().on_window_icon_name = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when BEL requests an audible bell.
    pub fn on_audible_bell<F>(self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.shared.lock().on_audible_bell = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 52 requests a clipboard copy.
    pub fn on_clipboard_copy<F>(self, callback: F) -> Self
    where
        F: Fn(&TerminalClipboardCopy) + Send + Sync + 'static,
    {
        self.shared.lock().on_clipboard_copy = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 133 marks a command as finished.
    pub fn on_command_finished<F>(self, callback: F) -> Self
    where
        F: Fn(&TerminalCommandBlock) + Send + Sync + 'static,
    {
        self.shared.lock().on_command_finished = Some(Arc::new(callback));
        self
    }

    /// Spawns a subprocess attached to the terminal's PTY.
    pub fn spawn_process(&mut self, command: &str, args: &[String]) -> Result<()> {
        let mut cmd = CommandBuilder::new(command);
        for arg in args {
            cmd.arg(arg);
        }
        self.spawn_command(cmd)
    }

    /// Spawns a subprocess using a custom command builder.
    pub fn spawn_command(&mut self, cmd: CommandBuilder) -> Result<()> {
        self.stop_process();
        {
            let mut shared = self.shared.lock();
            shared.exit_status = None;
            shared.process_running = false;
        }

        let (rows, cols) = {
            let shared = self.shared.lock();
            shared.parser.screen().size()
        };

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let child = pair.slave.spawn_command(cmd)?;
        let child = Arc::new(Mutex::new(child));
        let writer = pair.master.take_writer()?;
        let reader = pair.master.try_clone_reader()?;
        self.shared.lock().process_running = true;

        let handle = self.handle();
        let shared_for_reader = Arc::clone(&self.shared);
        let child_for_reader = Arc::clone(&child);
        let reader_alive = Arc::new(AtomicBool::new(true));
        let reader_alive_thread = Arc::clone(&reader_alive);
        let reader_thread = thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            while reader_alive_thread.load(Ordering::Relaxed) {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => handle.process_output(&buf[..n]),
                    Err(_) => break,
                }
            }
            if reader_alive_thread.load(Ordering::Relaxed) {
                try_record_child_exit(&shared_for_reader, &child_for_reader);
            }
        });
        let shared_for_watcher = Arc::clone(&self.shared);
        let child_for_watcher = Arc::clone(&child);
        let exit_watcher_alive = Arc::clone(&reader_alive);
        let exit_watcher_thread = thread::spawn(move || {
            while exit_watcher_alive.load(Ordering::Relaxed) {
                if try_record_child_exit(&shared_for_watcher, &child_for_watcher) {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });

        let forward_writer = Arc::new(Mutex::new(writer));
        self.shared.lock().input_forward = Some(Arc::new(move |bytes| {
            if bytes.is_empty() {
                return;
            }
            let mut writer = forward_writer.lock();
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }));

        self.process = Some(TerminalProcess {
            master: pair.master,
            child,
            reader_alive,
            reader_thread: Some(reader_thread),
            exit_watcher_thread: Some(exit_watcher_thread),
            last_size: (rows, cols),
        });

        Ok(())
    }

    /// Stops the currently attached subprocess, if any.
    pub fn stop_process(&mut self) {
        {
            let mut shared = self.shared.lock();
            shared.input_forward = None;
            shared.process_running = false;
        }
        if let Some(mut process) = self.process.take() {
            process.shutdown(&self.shared);
        }
    }

    fn handle_scrollback_wheel(&mut self, event: MouseEvent, step: u16) -> bool {
        let mut shared = self.shared.lock();
        let delta = match event.kind {
            MouseEventKind::ScrollUp => -(step as i16),
            MouseEventKind::ScrollDown => step as i16,
            _ => return false,
        };
        let max = shared.max_scrollback();
        let current = shared.parser.screen().scrollback();
        let desired = if delta.is_negative() {
            let amount = i32::from(delta).unsigned_abs() as usize;
            current.saturating_add(amount).min(max)
        } else {
            current.saturating_sub(delta as usize)
        };
        if desired != current {
            shared.parser.screen_mut().set_scrollback(desired);
            return true;
        }
        false
    }

    fn handle_alternate_screen_wheel(&mut self, event: MouseEvent, step: u16) -> bool {
        let key = match event.kind {
            MouseEventKind::ScrollUp => KeyCode::Up,
            MouseEventKind::ScrollDown => KeyCode::Down,
            _ => return false,
        };
        let shared = self.shared.lock();
        let screen = shared.parser.screen();
        if !matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None)
            || !screen.alternate_screen()
        {
            return false;
        }

        let Some(key_bytes) = encode_key_event(screen, KeyEvent::new(key, KeyModifiers::NONE))
        else {
            return true;
        };
        let mut bytes = Vec::with_capacity(key_bytes.len() * usize::from(step.max(1)));
        for _ in 0..step.max(1) {
            bytes.extend_from_slice(&key_bytes);
        }
        drop(shared);
        dispatch_input(&self.shared, &bytes);
        true
    }

    fn handle_scrollback_key(&mut self, event: KeyEvent) -> bool {
        if event.kind == KeyEventKind::Release {
            return false;
        }
        let mut shared = self.shared.lock();
        let max = shared.max_scrollback();
        let current = shared.parser.screen().scrollback();
        let rows = shared.parser.screen().size().0 as usize;
        let desired = match event.code {
            KeyCode::PageUp => current.saturating_add(rows).min(max),
            KeyCode::PageDown => current.saturating_sub(rows),
            KeyCode::Home => max,
            KeyCode::End => 0,
            _ => return false,
        };
        if desired != current {
            shared.parser.screen_mut().set_scrollback(desired);
            return true;
        }
        false
    }

    fn handle_local_mouse_selection(
        &mut self,
        event: MouseEvent,
        coordinate_space: MouseCoordinateSpace,
    ) -> bool {
        if !matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left)
        ) {
            return false;
        }

        let mut shared = self.shared.lock();
        let mouse_reporting_enabled = !matches!(
            shared.parser.screen().mouse_protocol_mode(),
            vt100::MouseProtocolMode::None
        );
        let selection_requested = !mouse_reporting_enabled
            || event.modifiers.contains(KeyModifiers::SHIFT)
            || shared.selection.is_dragging();
        if !selection_requested {
            return false;
        }

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    false,
                ) else {
                    return false;
                };
                shared.selection.start(position);
                true
            }
            MouseEventKind::Drag(MouseButton::Left) if shared.selection.is_dragging() => {
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    true,
                ) else {
                    return false;
                };
                shared.selection.update(position);
                true
            }
            MouseEventKind::Up(MouseButton::Left) if shared.selection.is_dragging() => {
                let include_cell = shared.selection.range().is_some();
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    include_cell,
                ) else {
                    return false;
                };
                shared.selection.finish(position);
                let text = shared.copy_selection();
                drop(shared);
                if let Some(text) = text {
                    dispatch_system_clipboard_copy(&self.shared, &text);
                }
                true
            }
            _ => false,
        }
    }

    fn handle_copy_mode_mouse(&mut self, event: MouseEvent) -> bool {
        let mut shared = self.shared.lock();
        if shared.copy_mode.is_none() {
            return false;
        }
        match event.kind {
            MouseEventKind::ScrollUp => {
                let _ = shared.scroll_copy_mode_view(self.scroll_step as isize);
            }
            MouseEventKind::ScrollDown => {
                let _ = shared.scroll_copy_mode_view(-(self.scroll_step as isize));
            }
            _ => {}
        }
        true
    }
}

impl Default for TerminalEmulator {
    fn default() -> Self {
        Self::new()
    }
}

impl ::atto_ui::composable::Component for TerminalEmulator {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }
        if let Some(process) = &mut self.process {
            process.record_exit_if_ready(&self.shared);
        }

        let mut shared = self.shared.lock();
        if !ctx.is_focused && shared.capture {
            shared.set_capture(false);
        }
        let (rows, cols) = (area.height, area.width);
        {
            let screen = shared.parser.screen_mut();
            if screen.size() != (rows, cols) {
                screen.set_size(rows, cols);
            }
        }
        if let Some(process) = &mut self.process {
            process.resize_if_needed(rows, cols);
        }
        let selection_range = shared.selection.range();
        let copy_mode_cursor = shared.copy_mode.as_ref().map(|mode| mode.cursor);
        let command_blocks = if self.command_block_presentation.is_enabled() {
            shared.command_marks.clone()
        } else {
            Vec::new()
        };
        let max_scrollback = shared.max_scrollback();
        let screen = shared.parser.screen_mut();
        let visible_top = visible_top_row(max_scrollback, screen.scrollback());

        let base_style = ctx.theme.window_bg;
        let base_fg = base_style.fg;
        let base_bg = base_style.bg;
        let command_output_style = command_output_style(ctx.theme);
        let command_separator_style = command_separator_style(ctx.theme);
        let command_failure_style = command_failure_style(ctx.theme);

        let buf = frame.buffer_mut();
        for y in 0..area.height {
            let absolute_row = visible_top.saturating_add(usize::from(y));
            let row_presentation = command_row_presentation(&command_blocks, absolute_row);
            let separator_start = if row_presentation.separator {
                command_separator_start(screen, y, area.width)
            } else {
                area.width
            };
            let selected_ranges = selection_range
                .map(|range| {
                    selected_cell_ranges_for_screen_row(screen, y, absolute_row, area.width, range)
                })
                .unwrap_or_default();
            for x in 0..area.width {
                let cell = screen.cell(y, x);
                let mut is_wide_cont = cell.is_some_and(vt100::Cell::is_wide_continuation);
                let mut symbol = cell
                    .map(|c| {
                        if c.is_wide_continuation() || c.contents().is_empty() {
                            " "
                        } else {
                            c.contents()
                        }
                    })
                    .unwrap_or(" ");

                let style = cell
                    .map(|c| cell_style(c, base_fg, base_bg))
                    .unwrap_or(base_style);
                let style = if row_presentation.output {
                    style.patch(command_output_style)
                } else {
                    style
                };
                let style = if row_presentation.separator && x >= separator_start && !is_wide_cont {
                    symbol = COMMAND_SEPARATOR_SYMBOL;
                    style.patch(command_separator_style)
                } else {
                    style
                };
                let style = if row_presentation.failed_marker && x == area.width.saturating_sub(1) {
                    symbol = COMMAND_FAILURE_SYMBOL;
                    is_wide_cont = false;
                    style.patch(command_failure_style)
                } else {
                    style
                };
                let style = if selected_ranges
                    .iter()
                    .any(|(start, end)| x >= *start && x < *end)
                {
                    ctx.theme.selection
                } else {
                    style
                };
                let style = if copy_mode_cursor.is_some_and(|cursor| {
                    absolute_row == cursor.row && x == cursor.col.min(area.width.saturating_sub(1))
                }) {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                };

                let dst_x = area.x.saturating_add(x);
                let dst_y = area.y.saturating_add(y);
                if let Some(dst) = buf.cell_mut((dst_x, dst_y)) {
                    dst.set_symbol(symbol);
                    dst.set_style(style);
                    dst.set_skip(is_wide_cont);
                }
            }
        }

        if !screen.hide_cursor() && screen.scrollback() == 0 {
            let (cur_row, cur_col) = screen.cursor_position();
            if cur_row < area.height && cur_col < area.width {
                let dst_x = area.x.saturating_add(cur_col);
                let dst_y = area.y.saturating_add(cur_row);
                if let Some(dst) = buf.cell_mut((dst_x, dst_y)) {
                    dst.set_style(dst.style().add_modifier(Modifier::REVERSED));
                }
            }
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for TerminalEmulator {}

impl ::atto_ui::composable::Layout for TerminalEmulator {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }
}

impl ::atto_ui::composable::Scrollable for TerminalEmulator {
    fn is_scrollable(&self) -> bool {
        let mut shared = self.shared.lock();
        let max = shared.max_scrollback();
        max > 0
    }

    fn content_size(&self) -> (u16, u16) {
        let mut shared = self.shared.lock();
        let (rows, cols) = shared.parser.screen().size();
        let max = shared.max_scrollback().min(u16::MAX as usize);
        let height = rows.saturating_add(max as u16);
        (cols, height)
    }

    fn viewport_size(&self) -> (u16, u16) {
        let shared = self.shared.lock();
        let (rows, cols) = shared.parser.screen().size();
        (cols, rows)
    }

    fn scroll_config(&self) -> ScrollConfig {
        ScrollConfig::default()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let mut shared = self.shared.lock();
        let max = shared.max_scrollback().min(u16::MAX as usize);
        let offset = shared.scrollback_offset().min(max);
        let y = max.saturating_sub(offset) as u16;
        (0, y)
    }

    fn set_scroll_offset(&mut self, _x: u16, y: u16) {
        let mut shared = self.shared.lock();
        shared.set_scrollback_from_scroll_offset(y);
    }
}

impl ::atto_ui::composable::FocusNav for TerminalEmulator {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl ::atto_ui::composable::DynamicTree for TerminalEmulator {}

impl ::atto_ui::composable::EventHandling for TerminalEmulator {
    fn handle_event_capture(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        let Event::Key(key) = event else {
            return EventResult::ignored();
        };
        let mut shared = self.shared.lock();
        if !shared.capture {
            return EventResult::ignored();
        }
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => match handle_captured_key(&mut shared, *key) {
                CapturedKeyAction::Consumed => EventResult::consumed(),
                CapturedKeyAction::Component(action) => EventResult {
                    outcome: EventOutcome::Consumed,
                    action,
                    capture: Capture::None,
                },
                CapturedKeyAction::Dispatch(bytes) => {
                    drop(shared);
                    dispatch_input(&self.shared, &bytes);
                    EventResult::consumed()
                }
                CapturedKeyAction::SystemClipboardCopy(text) => {
                    drop(shared);
                    dispatch_system_clipboard_copy(&self.shared, &text);
                    EventResult::consumed()
                }
            },
            _ => EventResult::ignored(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Key(key) => {
                let mut shared = self.shared.lock();
                if shared.capture {
                    match handle_captured_key(&mut shared, *key) {
                        CapturedKeyAction::Consumed => return EventResult::consumed(),
                        CapturedKeyAction::Component(action) => {
                            return EventResult {
                                outcome: EventOutcome::Consumed,
                                action,
                                capture: Capture::None,
                            };
                        }
                        CapturedKeyAction::Dispatch(bytes) => {
                            drop(shared);
                            dispatch_input(&self.shared, &bytes);
                            return EventResult::consumed();
                        }
                        CapturedKeyAction::SystemClipboardCopy(text) => {
                            drop(shared);
                            dispatch_system_clipboard_copy(&self.shared, &text);
                            return EventResult::consumed();
                        }
                    }
                }
                drop(shared);
                if self.handle_scrollback_key(*key) {
                    return EventResult::consumed();
                }
                EventResult::ignored()
            }
            Event::Paste(text) => {
                let shared = self.shared.lock();
                if !shared.capture {
                    return EventResult::ignored();
                }
                let bytes = encode_paste_text(shared.parser.screen(), text);
                drop(shared);
                dispatch_input(&self.shared, &bytes);
                EventResult::consumed()
            }
            Event::Mouse(m) => {
                let inside =
                    mouse_coords_local(self.last_area, *m, ctx.mouse_coordinate_space).is_some();
                if !inside {
                    return EventResult::ignored();
                }

                let mut shared = self.shared.lock();
                if !shared.capture {
                    if matches!(m.kind, MouseEventKind::Down(_)) && self.capture_on_click {
                        shared.set_capture(true);
                    } else {
                        drop(shared);
                        if self.handle_scrollback_wheel(*m, self.scroll_step) {
                            return EventResult::consumed();
                        }
                        return EventResult::ignored();
                    }
                }
                drop(shared);

                if self.handle_copy_mode_mouse(*m) {
                    return EventResult::consumed();
                }

                if self.handle_local_mouse_selection(*m, ctx.mouse_coordinate_space) {
                    return EventResult::consumed();
                }

                let shared = self.shared.lock();
                let screen = shared.parser.screen();
                if let Some(bytes) =
                    encode_mouse_event(screen, *m, self.last_area, ctx.mouse_coordinate_space)
                {
                    drop(shared);
                    dispatch_input(&self.shared, &bytes);
                    return EventResult::consumed();
                }
                drop(shared);
                if self.handle_alternate_screen_wheel(*m, self.scroll_step) {
                    return EventResult::consumed();
                }
                if self.handle_scrollback_wheel(*m, self.scroll_step) {
                    return EventResult::consumed();
                }
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }
}

impl Drop for TerminalEmulator {
    fn drop(&mut self) {
        self.stop_process();
        if let Some(cb) = self.on_close.take() {
            cb();
        }
    }
}

/// Handle for interacting with a [`TerminalEmulator`] from outside the UI tree.
#[derive(Clone)]
pub struct TerminalHandle {
    shared: Arc<Mutex<TerminalShared>>,
}

impl TerminalHandle {
    /// Feeds bytes into the terminal emulator (ANSI output stream).
    pub fn process_output(&self, bytes: &[u8]) {
        let (responses, dispatches) = {
            let mut shared = self.shared.lock();
            shared.parser.process(bytes);
            let events = shared.parser.callbacks_mut().take_events();
            let responses = collect_dsr_responses(&mut shared, bytes);
            let dispatches = shared.apply_callback_events(events);
            (responses, dispatches)
        };
        for response in responses {
            forward_input(&self.shared, &response);
        }
        dispatch_terminal_callback_events(&self.shared, dispatches);
    }

    pub fn process_output_str(&self, text: &str) {
        self.process_output(text.as_bytes());
    }

    /// Sends a user input event to the terminal (encoded to bytes).
    pub fn send_event(&self, event: &Event) {
        let shared = self.shared.lock();
        let screen = shared.parser.screen();
        let bytes = match event {
            Event::Key(key) => encode_key_event(screen, *key),
            Event::Paste(text) => Some(encode_paste_text(screen, text)),
            Event::Mouse(m) => encode_mouse_event(screen, *m, None, MouseCoordinateSpace::Absolute),
            _ => None,
        };
        if let Some(bytes) = bytes {
            drop(shared);
            dispatch_input(&self.shared, &bytes);
        }
    }

    /// Pushes raw input bytes to the terminal input stream.
    pub fn send_input_bytes(&self, bytes: &[u8]) {
        dispatch_input(&self.shared, bytes);
    }

    /// Returns and clears the queued input bytes.
    pub fn take_input(&self) -> Vec<u8> {
        let mut shared = self.shared.lock();
        let mut out = Vec::with_capacity(shared.input.len());
        while let Some(b) = shared.input.pop_front() {
            out.push(b);
        }
        out
    }

    pub fn set_capture(&self, capture: bool) {
        self.shared.lock().set_capture(capture);
    }

    pub fn capture(&self) -> bool {
        self.shared.lock().capture
    }

    pub fn set_release_shortcut(&self, shortcut: TerminalShortcut) {
        self.shared.lock().release_shortcut = shortcut;
    }

    pub fn release_shortcut(&self) -> TerminalShortcut {
        self.shared.lock().release_shortcut
    }

    /// Updates the terminal prefix shortcut. Only plain `Ctrl+<ASCII letter>` is accepted.
    pub fn set_prefix_shortcut(&self, shortcut: TerminalShortcut) -> Result<()> {
        let shortcut = normalize_prefix_shortcut(shortcut)?;
        self.shared.lock().set_prefix_shortcut(shortcut);
        Ok(())
    }

    pub fn prefix_shortcut(&self) -> TerminalShortcut {
        self.shared.lock().prefix_shortcut
    }

    /// Adds or replaces one prefix command binding at runtime.
    pub fn set_prefix_binding(&self, shortcut: TerminalShortcut, command: TerminalPrefixCommand) {
        self.shared
            .lock()
            .set_prefix_binding(TerminalPrefixBinding::new(shortcut, command));
    }

    /// Replaces the full prefix command table at runtime.
    pub fn set_prefix_bindings(&self, bindings: impl IntoIterator<Item = TerminalPrefixBinding>) {
        self.shared.lock().set_prefix_bindings(bindings);
    }

    pub fn prefix_bindings(&self) -> Vec<TerminalPrefixBinding> {
        self.shared.lock().prefix_bindings.clone()
    }

    /// Returns whether copy-mode is currently active.
    pub fn copy_mode(&self) -> bool {
        self.shared.lock().copy_mode.is_some()
    }

    /// Returns the current copy-mode cursor position, if copy-mode is active.
    pub fn copy_mode_cursor(&self) -> Option<TerminalSelectionPosition> {
        self.shared
            .lock()
            .copy_mode
            .as_ref()
            .map(|mode| mode.cursor)
    }

    /// Returns the last text copied into the terminal-local copy buffer.
    pub fn copied_text(&self) -> Option<String> {
        self.shared.lock().copy_buffer.clone()
    }

    /// Copies the active selection into the terminal-local copy buffer.
    pub fn copy_selection(&self) -> Option<String> {
        let text = { self.shared.lock().copy_selection() };
        if let Some(text) = &text {
            dispatch_system_clipboard_copy(&self.shared, text);
        }
        text
    }

    /// Pastes the terminal-local copy buffer back into the subprocess input stream.
    pub fn paste_copied_text(&self) -> bool {
        let bytes = { self.shared.lock().paste_copy_buffer_bytes() };
        let Some(bytes) = bytes else {
            return false;
        };
        dispatch_input(&self.shared, &bytes);
        true
    }

    /// Starts a terminal text selection at an absolute scrollback/screen position.
    pub fn begin_selection(&self, position: TerminalSelectionPosition) {
        self.shared.lock().selection.start(position);
    }

    /// Extends the active terminal text selection to an absolute position.
    pub fn update_selection(&self, position: TerminalSelectionPosition) {
        self.shared.lock().selection.update(position);
    }

    /// Clears the active terminal text selection.
    pub fn clear_selection(&self) -> bool {
        self.shared.lock().selection.clear()
    }

    /// Returns the normalized active terminal text selection range.
    pub fn selection_range(&self) -> Option<TerminalSelectionRange> {
        self.shared.lock().selection.range()
    }

    /// Converts a visible terminal cell into the absolute coordinate used by selections.
    pub fn selection_position_for_view_cell(
        &self,
        row: u16,
        col: u16,
    ) -> TerminalSelectionPosition {
        let mut shared = self.shared.lock();
        let max_scrollback = shared.max_scrollback();
        let screen = shared.parser.screen();
        let (rows, cols) = screen.size();
        position_for_view_cell(max_scrollback, screen.scrollback(), rows, cols, row, col)
    }

    /// Returns text currently covered by the active selection.
    pub fn selected_text(&self) -> Option<String> {
        let mut shared = self.shared.lock();
        shared.selected_text()
    }

    /// Returns the latest OSC 0/2 window title, if one has been observed.
    pub fn window_title(&self) -> Option<String> {
        self.shared.lock().window_title.clone()
    }

    /// Returns the latest OSC 0/1 window icon name, if one has been observed.
    pub fn window_icon_name(&self) -> Option<String> {
        self.shared.lock().window_icon_name.clone()
    }

    /// Returns the number of audible bell requests observed in terminal output.
    pub fn audible_bell_count(&self) -> u64 {
        self.shared.lock().audible_bell_count
    }

    /// Returns the latest OSC 52 clipboard-copy request, if one has been observed.
    pub fn last_clipboard_copy(&self) -> Option<TerminalClipboardCopy> {
        self.shared.lock().last_clipboard_copy.clone()
    }

    /// Returns the last text sent to the configured system clipboard backend.
    pub fn last_system_clipboard_text(&self) -> Option<String> {
        self.shared.lock().last_system_clipboard_text.clone()
    }

    /// Returns the last system clipboard sync error, if the most recent sync failed.
    pub fn last_system_clipboard_error(&self) -> Option<String> {
        self.shared.lock().last_system_clipboard_error.clone()
    }

    /// Returns OSC 133/7 command blocks observed in terminal output.
    pub fn command_blocks(&self) -> Vec<TerminalCommandBlock> {
        self.shared.lock().command_marks.clone()
    }

    /// Returns the exit code for the most recently completed command block, if reported.
    pub fn last_exit_code(&self) -> Option<i32> {
        self.shared
            .lock()
            .command_marks
            .iter()
            .rev()
            .find(|block| block.end.is_some())
            .and_then(|block| block.exit_code)
    }

    /// Returns whether a subprocess is currently attached and has not reported exit.
    pub fn is_running(&self) -> bool {
        self.shared.lock().process_running
    }

    /// Returns the last recorded subprocess exit status, if the process has exited.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.shared.lock().exit_status.clone()
    }

    /// Snapshot of the terminal contents including scrollback.
    pub fn snapshot(&self) -> TerminalSnapshot {
        let mut shared = self.shared.lock();
        let max_scrollback = shared.max_scrollback();
        let (rows, cols, current_offset) = {
            let screen = shared.parser.screen();
            let (rows, cols) = screen.size();
            let current_offset = screen.scrollback();
            (rows, cols, current_offset)
        };
        let screen = shared.parser.screen_mut();

        let mut lines = Vec::with_capacity(max_scrollback + rows as usize);
        let mut start = 0;
        while start < max_scrollback {
            let offset = max_scrollback - start;
            screen.set_scrollback(offset);
            let chunk: Vec<String> = screen.rows(0, cols).collect();
            let take = (max_scrollback - start).min(rows as usize);
            lines.extend(chunk.into_iter().take(take));
            start += take;
        }

        screen.set_scrollback(0);
        lines.extend(screen.rows(0, cols));

        screen.set_scrollback(current_offset);

        TerminalSnapshot {
            lines,
            cols,
            rows,
            scrollback: max_scrollback,
        }
    }
}

/// Full text snapshot of the terminal contents including scrollback.
#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    pub lines: Vec<String>,
    pub cols: u16,
    pub rows: u16,
    pub scrollback: usize,
}

impl TerminalSnapshot {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

fn mouse_coords_local(
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

fn mouse_selection_position(
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

fn cell_style(cell: &vt100::Cell, base_fg: Option<Color>, base_bg: Option<Color>) -> Style {
    let mut fg = resolve_color(cell.fgcolor(), base_fg);
    let mut bg = resolve_color(cell.bgcolor(), base_bg);
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

fn resolve_color(color: vt100::Color, default: Option<Color>) -> Option<Color> {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) => Some(color_from_index(i)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

fn color_from_index(i: u8) -> Color {
    match i {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        15 => Color::White,
        _ => Color::Indexed(i),
    }
}

fn encode_key_event(screen: &vt100::Screen, event: KeyEvent) -> Option<Vec<u8>> {
    if event.kind == KeyEventKind::Release {
        return None;
    }

    let mods = event.modifiers;
    let shift = mods.contains(KeyModifiers::SHIFT);
    let alt = mods.contains(KeyModifiers::ALT);
    let ctrl = mods.contains(KeyModifiers::CONTROL);

    let mut out = Vec::new();

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

fn encode_cursor_key(screen: &vt100::Screen, suffix: char, mods: KeyModifiers) -> String {
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

fn encode_home_end_key(screen: &vt100::Screen, suffix: char, mods: KeyModifiers) -> String {
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

fn encode_csi_tilde(n: u8, mods: KeyModifiers) -> String {
    let mod_value = modifier_value(mods);
    if mod_value == 1 {
        format!("\x1b[{n}~")
    } else {
        format!("\x1b[{n};{mod_value}~")
    }
}

fn encode_function_key(n: u8, mods: KeyModifiers) -> Option<String> {
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

fn modifier_value(mods: KeyModifiers) -> u8 {
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

fn ctrl_char(c: char) -> Option<u8> {
    let c = c.to_ascii_uppercase();
    match c {
        '@' => Some(0),
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

fn encode_mouse_event(
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
        vt100::MouseProtocolEncoding::Utf8 | vt100::MouseProtocolEncoding::Default => {
            let cb = if matches!(event.kind, MouseEventKind::Up(_)) {
                3 + modifier_bits
            } else {
                cb
            };
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
    }
}
