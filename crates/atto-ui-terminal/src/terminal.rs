use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, ExitStatus, PtySize, native_pty_system};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use atto_ui::composable::{ComponentContext, EventResult, MouseCoordinateSpace, ScrollConfig};

const DEFAULT_SCROLLBACK_LEN: usize = 2000;
const DEFAULT_SCROLL_STEP: u16 = 3;

/// Keyboard shortcut used to release terminal input capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalShortcut {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
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
            exit_status: None,
            process_running: false,
            window_title: None,
            window_icon_name: None,
            audible_bell_count: 0,
            last_clipboard_copy: None,
            capture: true,
            release_shortcut: default_release_shortcut(),
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
}

fn default_release_shortcut() -> TerminalShortcut {
    TerminalShortcut {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    }
}

type InputCallback = Arc<dyn Fn(&[u8]) + Send + Sync>;
type ExitCallback = Arc<dyn Fn(ExitStatus) + Send + Sync>;
type TextCallback = Arc<dyn Fn(&str) + Send + Sync>;
type BellCallback = Arc<dyn Fn() + Send + Sync>;
type ClipboardCopyCallback = Arc<dyn Fn(&TerminalClipboardCopy) + Send + Sync>;
type TerminalParser = vt100::Parser<TerminalCallbacks>;

/// OSC 52 clipboard-copy request observed in the terminal output stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalClipboardCopy {
    /// Clipboard selector from the OSC 52 sequence, for example `c`.
    pub selector: Vec<u8>,
    /// Base64-encoded clipboard payload from the OSC 52 sequence.
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalCallbackEvent {
    WindowTitle(String),
    WindowIconName(String),
    AudibleBell,
    ClipboardCopy(TerminalClipboardCopy),
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
}

enum TerminalCallbackDispatch {
    WindowTitle(TextCallback, String),
    WindowIconName(TextCallback, String),
    AudibleBell(BellCallback),
    ClipboardCopy(ClipboardCopyCallback, TerminalClipboardCopy),
}

fn terminal_parser(rows: u16, cols: u16, scrollback_len: usize) -> TerminalParser {
    vt100::Parser::new_with_callbacks(rows, cols, scrollback_len, TerminalCallbacks::default())
}

fn string_from_terminal_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
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
    exit_status: Option<ExitStatus>,
    process_running: bool,
    window_title: Option<String>,
    window_icon_name: Option<String>,
    audible_bell_count: u64,
    last_clipboard_copy: Option<TerminalClipboardCopy>,
    capture: bool,
    release_shortcut: TerminalShortcut,
    dsr_tail: Vec<u8>,
}

impl TerminalShared {
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
                    if let Some(callback) = self.on_clipboard_copy.clone() {
                        dispatches.push(TerminalCallbackDispatch::ClipboardCopy(callback, copy));
                    }
                }
            }
        }
        dispatches
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

fn dispatch_terminal_callback_events(dispatches: Vec<TerminalCallbackDispatch>) {
    for dispatch in dispatches {
        match dispatch {
            TerminalCallbackDispatch::WindowTitle(callback, title) => callback(&title),
            TerminalCallbackDispatch::WindowIconName(callback, icon_name) => callback(&icon_name),
            TerminalCallbackDispatch::AudibleBell(callback) => callback(),
            TerminalCallbackDispatch::ClipboardCopy(callback, copy) => callback(&copy),
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
            exit_status: None,
            process_running: false,
            window_title: None,
            window_icon_name: None,
            audible_bell_count: 0,
            last_clipboard_copy: None,
            capture: true,
            release_shortcut: default_release_shortcut(),
            dsr_tail: Vec::with_capacity(4),
        };

        Self {
            shared: Arc::new(Mutex::new(shared)),
            last_area: None,
            scroll_step: DEFAULT_SCROLL_STEP,
            capture_on_click: true,
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
        self.shared.lock().capture = capture;
        self
    }

    pub fn release_shortcut(self, shortcut: TerminalShortcut) -> Self {
        self.shared.lock().release_shortcut = shortcut;
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
            shared.capture = false;
        }
        let screen = shared.parser.screen_mut();
        let (rows, cols) = (area.height, area.width);
        if screen.size() != (rows, cols) {
            screen.set_size(rows, cols);
        }
        if let Some(process) = &mut self.process {
            process.resize_if_needed(rows, cols);
        }

        let base_style = ctx.theme.window_bg;
        let base_fg = base_style.fg;
        let base_bg = base_style.bg;

        let buf = frame.buffer_mut();
        for y in 0..area.height {
            for x in 0..area.width {
                let cell = screen.cell(y, x);
                let is_wide_cont = cell.is_some_and(vt100::Cell::is_wide_continuation);
                let symbol = cell
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
        let shared = self.shared.lock();
        if !shared.capture {
            return EventResult::ignored();
        }
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                if let Some(bytes) = encode_key_event(shared.parser.screen(), *key) {
                    drop(shared);
                    dispatch_input(&self.shared, &bytes);
                    return EventResult::consumed();
                }
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Key(key) => {
                let mut shared = self.shared.lock();
                if shared.capture {
                    if shared.release_shortcut.matches(*key) {
                        shared.capture = false;
                        return EventResult::consumed();
                    }
                    if let Some(bytes) = encode_key_event(shared.parser.screen(), *key) {
                        drop(shared);
                        dispatch_input(&self.shared, &bytes);
                        return EventResult::consumed();
                    }
                    return EventResult::consumed();
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
                let screen = shared.parser.screen();
                let bytes = if screen.bracketed_paste() {
                    let mut buf = Vec::with_capacity(text.len() + 16);
                    buf.extend_from_slice(b"\x1b[200~");
                    buf.extend_from_slice(text.as_bytes());
                    buf.extend_from_slice(b"\x1b[201~");
                    buf
                } else {
                    text.as_bytes().to_vec()
                };
                drop(shared);
                dispatch_input(&self.shared, &bytes);
                EventResult::consumed()
            }
            Event::Mouse(m) => {
                let shared = self.shared.lock();
                let screen = shared.parser.screen();
                let inside =
                    mouse_coords_local(self.last_area, *m, ctx.mouse_coordinate_space).is_some();
                if !inside {
                    return EventResult::ignored();
                }

                if !shared.capture {
                    drop(shared);
                    if matches!(m.kind, MouseEventKind::Down(_)) && self.capture_on_click {
                        self.shared.lock().capture = true;
                        return EventResult::consumed();
                    }
                    if self.handle_scrollback_wheel(*m, self.scroll_step) {
                        return EventResult::consumed();
                    }
                    return EventResult::ignored();
                }

                if let Some(bytes) =
                    encode_mouse_event(screen, *m, self.last_area, ctx.mouse_coordinate_space)
                {
                    drop(shared);
                    dispatch_input(&self.shared, &bytes);
                    return EventResult::consumed();
                }
                drop(shared);
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
        dispatch_terminal_callback_events(dispatches);
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
            Event::Paste(text) => {
                if screen.bracketed_paste() {
                    let mut buf = Vec::with_capacity(text.len() + 16);
                    buf.extend_from_slice(b"\x1b[200~");
                    buf.extend_from_slice(text.as_bytes());
                    buf.extend_from_slice(b"\x1b[201~");
                    Some(buf)
                } else {
                    Some(text.as_bytes().to_vec())
                }
            }
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
        self.shared.lock().capture = capture;
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
