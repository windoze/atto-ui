#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
pub use crossterm::event::{KeyCode, KeyModifiers};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreenRegion {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl ScreenRegion {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

impl From<(u16, u16, u16, u16)> for ScreenRegion {
    fn from((x, y, width, height): (u16, u16, u16, u16)) -> Self {
        Self::new(x, y, width, height)
    }
}

pub struct PtyTestHost {
    child: Box<dyn portable_pty::Child + Send>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    raw_output: Arc<Mutex<Vec<u8>>>,
    read_thread: Option<JoinHandle<Result<()>>>,
    cols: u16,
    rows: u16,
}

impl PtyTestHost {
    pub fn spawn(program: impl AsRef<Path>, args: &[&str], cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("openpty")?;

        let mut cmd = CommandBuilder::new(program.as_ref());
        for a in args {
            cmd.arg(a);
        }
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd).context("spawn_command")?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().context("try_clone_reader")?;
        let writer = pair.master.take_writer().context("take_writer")?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let parser_for_thread = Arc::clone(&parser);
        let raw_output = Arc::new(Mutex::new(Vec::new()));
        let raw_output_for_thread = Arc::clone(&raw_output);

        let read_thread = thread::spawn(move || -> Result<()> {
            let mut buf = [0u8; 16 * 1024];
            loop {
                let n = reader.read(&mut buf).context("pty read")?;
                if n == 0 {
                    break;
                }
                raw_output_for_thread
                    .lock()
                    .map_err(|_| anyhow!("raw output buffer poisoned"))?
                    .extend_from_slice(&buf[..n]);
                let mut p = parser_for_thread
                    .lock()
                    .map_err(|_| anyhow!("parser poisoned"))?;
                p.process(&buf[..n]);
            }
            Ok(())
        });

        Ok(Self {
            child,
            master: pair.master,
            writer,
            parser,
            raw_output,
            read_thread: Some(read_thread),
            cols,
            rows,
        })
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes).context("pty write")?;
        self.writer.flush().context("pty flush")?;
        Ok(())
    }

    pub fn send_str(&mut self, s: &str) -> Result<()> {
        self.send(s.as_bytes())
    }

    pub fn send_ctrl(&mut self, c: char) -> Result<()> {
        let b = (c as u8) & 0x1f;
        self.send(&[b])
    }

    /// Sends a bracketed paste sequence (requires the app to enable bracketed paste).
    pub fn send_paste(&mut self, s: &str) -> Result<()> {
        self.send(b"\x1b[200~")?;
        self.send(s.as_bytes())?;
        self.send(b"\x1b[201~")?;
        Ok(())
    }

    /// Sends a left mouse click using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn click(&mut self, x: u16, y: u16) -> Result<()> {
        self.click_with_mods(x, y, KeyModifiers::NONE)
    }

    /// Sends a left mouse click with modifiers using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn click_with_mods(&mut self, x: u16, y: u16, mods: KeyModifiers) -> Result<()> {
        self.mouse_click(0, x, y, mods)
    }

    /// Sends a right mouse click using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn right_click(&mut self, x: u16, y: u16) -> Result<()> {
        self.mouse_click(2, x, y, KeyModifiers::NONE)
    }

    /// Sends a middle mouse click using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn middle_click(&mut self, x: u16, y: u16) -> Result<()> {
        self.mouse_click(1, x, y, KeyModifiers::NONE)
    }

    /// Sends a mouse move event with no button held using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn mouse_move(&mut self, x: u16, y: u16) -> Result<()> {
        self.sgr_mouse_event(35, x, y, true)
    }

    /// Sends a Shift + left mouse click using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn shift_click(&mut self, x: u16, y: u16) -> Result<()> {
        self.click_with_mods(x, y, KeyModifiers::SHIFT)
    }

    /// Sends a mouse wheel scroll event using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn wheel_up(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_event(64, x, y)
    }

    /// Sends a mouse wheel scroll event using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn wheel_down(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_event(65, x, y)
    }

    pub fn wheel_left(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_event(66, x, y)
    }

    pub fn wheel_right(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_event(67, x, y)
    }

    pub fn scroll_left(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_left(x, y)
    }

    pub fn scroll_right(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_right(x, y)
    }

    fn wheel_event(&mut self, cb: u16, x: u16, y: u16) -> Result<()> {
        self.sgr_mouse_event(cb, x, y, true)
    }

    /// Sends a mouse drag (left button) from `(x0, y0)` to `(x1, y1)` using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn drag_left(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<()> {
        let x0_1 = x0.saturating_add(1);
        let y0_1 = y0.saturating_add(1);
        let x1_1 = x1.saturating_add(1);
        let y1_1 = y1.saturating_add(1);

        let press = format!("\x1b[<0;{x0_1};{y0_1}M");
        // 32 means "drag" with button 1 held (left button: 0).
        let drag = format!("\x1b[<32;{x1_1};{y1_1}M");
        let release = format!("\x1b[<0;{x1_1};{y1_1}m");

        self.send_str(&press)?;
        self.send_str(&drag)?;
        self.send_str(&release)?;
        Ok(())
    }

    pub fn key_with_mods(&mut self, key: KeyCode, mods: KeyModifiers) -> Result<()> {
        let bytes = encode_key(key, mods)?;
        self.send(&bytes)
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        if cols == 0 || rows == 0 {
            return Err(anyhow!("PTY size must be non-zero, got {cols}x{rows}"));
        }

        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("pty resize")?;

        {
            let mut p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
            p.screen_mut().set_size(rows, cols);
        }
        self.cols = cols;
        self.rows = rows;
        Ok(())
    }

    fn mouse_click(&mut self, button_cb: u16, x: u16, y: u16, mods: KeyModifiers) -> Result<()> {
        let cb = button_cb + mouse_modifier_bits(mods)?;
        self.sgr_mouse_event(cb, x, y, true)?;
        self.sgr_mouse_event(cb, x, y, false)?;
        Ok(())
    }

    fn sgr_mouse_event(&mut self, cb: u16, x: u16, y: u16, press: bool) -> Result<()> {
        let x1 = x.saturating_add(1);
        let y1 = y.saturating_add(1);
        let suffix = if press { 'M' } else { 'm' };
        let seq = format!("\x1b[<{cb};{x1};{y1}{suffix}");
        self.send_str(&seq)
    }

    pub fn screen_contents(&self) -> Result<String> {
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        Ok(p.screen().contents().to_string())
    }

    /// Returns normalized visible screen rows.
    ///
    /// Each row is trimmed and trailing empty rows are removed so assertions are stable across
    /// terminal widths.
    pub fn screen_snapshot(&self) -> Result<Vec<String>> {
        self.region_snapshot(ScreenRegion::new(0, 0, self.cols, self.rows))
    }

    /// Returns normalized rows for a rectangular region of the visible screen.
    pub fn region_snapshot(&self, region: impl Into<ScreenRegion>) -> Result<Vec<String>> {
        let region = region.into();
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        let screen = p.screen();
        let (rows, cols) = screen.size();
        let end_x = region
            .x
            .checked_add(region.width)
            .ok_or_else(|| anyhow!("region width overflows u16"))?;
        let end_y = region
            .y
            .checked_add(region.height)
            .ok_or_else(|| anyhow!("region height overflows u16"))?;
        if end_x > cols || end_y > rows {
            return Err(anyhow!(
                "region {:?} out of bounds for screen {cols}x{rows}",
                region
            ));
        }

        let lines = screen
            .rows(region.x, region.width)
            .skip(usize::from(region.y))
            .take(usize::from(region.height))
            .map(|line| line.trim().to_string())
            .collect::<Vec<_>>();
        Ok(normalize_snapshot_lines(lines))
    }

    /// Returns the current cursor position as `(row, col)`, matching `vt100::Screen`.
    pub fn cursor_position(&self) -> Result<(u16, u16)> {
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        Ok(p.screen().cursor_position())
    }

    /// Returns all bytes emitted by the child process since spawn.
    pub fn raw_output(&self) -> Result<Vec<u8>> {
        Ok(self
            .raw_output
            .lock()
            .map_err(|_| anyhow!("raw output buffer poisoned"))?
            .clone())
    }

    /// Waits until the raw PTY output contains `needle`.
    pub fn wait_for_output(&self, needle: &[u8], timeout: Duration) -> Result<()> {
        if needle.is_empty() {
            return Ok(());
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let output = self.raw_output().unwrap_or_default();
            if output.windows(needle.len()).any(|window| window == needle) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = self.raw_output().unwrap_or_default();
        Err(anyhow!(
            "timed out waiting for raw output {:?}.\n--- output ---\n{}",
            String::from_utf8_lossy(needle),
            String::from_utf8_lossy(&output)
        ))
    }

    /// Returns the raw contents of the cell at `(x, y)` (0-based).
    ///
    /// This is more precise than scanning `screen_contents()` when wide characters are involved,
    /// since `vt100::Screen::contents()` is sparse and omits trailing whitespace.
    pub fn cell_contents(&self, x: u16, y: u16) -> Result<String> {
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        let screen = p.screen();
        let cell = screen
            .cell(y, x)
            .ok_or_else(|| anyhow!("cell ({x}, {y}) out of bounds"))?;
        Ok(cell.contents().to_string())
    }

    pub fn cell_fgcolor(&self, x: u16, y: u16) -> Result<vt100::Color> {
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        let screen = p.screen();
        let cell = screen
            .cell(y, x)
            .ok_or_else(|| anyhow!("cell ({x}, {y}) out of bounds"))?;
        Ok(cell.fgcolor())
    }

    pub fn cell_bgcolor(&self, x: u16, y: u16) -> Result<vt100::Color> {
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        let screen = p.screen();
        let cell = screen
            .cell(y, x)
            .ok_or_else(|| anyhow!("cell ({x}, {y}) out of bounds"))?;
        Ok(cell.bgcolor())
    }

    pub fn cell_inverse(&self, x: u16, y: u16) -> Result<bool> {
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        let screen = p.screen();
        let cell = screen
            .cell(y, x)
            .ok_or_else(|| anyhow!("cell ({x}, {y}) out of bounds"))?;
        Ok(cell.inverse())
    }

    pub fn cell_underlined(&self, x: u16, y: u16) -> Result<bool> {
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        let screen = p.screen();
        let cell = screen
            .cell(y, x)
            .ok_or_else(|| anyhow!("cell ({x}, {y}) out of bounds"))?;
        Ok(cell.underline())
    }

    pub fn wait_for_text(&self, needle: &str, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.screen_contents().unwrap_or_default().contains(needle) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        let snapshot = self.screen_contents().unwrap_or_default();
        Err(anyhow!(
            "timed out waiting for text {needle:?} in screen.\n--- screen ---\n{snapshot}"
        ))
    }

    pub fn wait_for_screen<F>(&self, mut pred: F, timeout: Duration) -> Result<Vec<String>>
    where
        F: FnMut(&[String]) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut last_snapshot = Vec::new();
        while Instant::now() < deadline {
            last_snapshot = self.screen_snapshot().unwrap_or_default();
            if pred(&last_snapshot) {
                return Ok(last_snapshot);
            }
            thread::sleep(Duration::from_millis(10));
        }
        Err(anyhow!(
            "timed out waiting for screen predicate.\n--- screen ---\n{}",
            last_snapshot.join("\n")
        ))
    }

    pub fn wait_for_exit(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().context("try_wait")? {
                if !status.success() {
                    return Err(anyhow!("child exited with status {status:?}"));
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        Err(anyhow!("timed out waiting for child exit"))
    }
}

fn normalize_snapshot_lines(mut lines: Vec<String>) -> Vec<String> {
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines
}

fn mouse_modifier_bits(mods: KeyModifiers) -> Result<u16> {
    let supported = KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL;
    let unsupported = mods & !supported;
    if !unsupported.is_empty() {
        return Err(anyhow!(
            "mouse SGR encoding supports only Shift/Alt/Control modifiers, got {mods:?}"
        ));
    }

    let mut cb = 0;
    if mods.contains(KeyModifiers::SHIFT) {
        cb += 4;
    }
    if mods.contains(KeyModifiers::ALT) {
        cb += 8;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        cb += 16;
    }
    Ok(cb)
}

fn key_modifier_param(mods: KeyModifiers) -> u8 {
    let mut param = 1;
    if mods.contains(KeyModifiers::SHIFT) {
        param += 1;
    }
    if mods.contains(KeyModifiers::ALT) {
        param += 2;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        param += 4;
    }
    if mods.contains(KeyModifiers::SUPER) {
        param += 8;
    }
    if mods.contains(KeyModifiers::HYPER) {
        param += 16;
    }
    if mods.contains(KeyModifiers::META) {
        param += 32;
    }
    param
}

fn encode_key(key: KeyCode, mods: KeyModifiers) -> Result<Vec<u8>> {
    if mods.is_empty() {
        return encode_key_without_mods(key);
    }

    if let Some((prefix, suffix)) = modifier_key_sequence_parts(key) {
        let param = key_modifier_param(mods);
        return Ok(format!("\x1b[{prefix};{param}{suffix}").into_bytes());
    }

    if let Some(code) = tilde_key_code(key) {
        let param = key_modifier_param(mods);
        return Ok(format!("\x1b[{code};{param}~").into_bytes());
    }

    if key == KeyCode::BackTab {
        return Ok(b"\x1b[Z".to_vec());
    }

    if let Some(codepoint) = csi_u_codepoint(key) {
        let param = key_modifier_param(mods);
        return Ok(format!("\x1b[{codepoint};{param}u").into_bytes());
    }

    Err(anyhow!(
        "unsupported modified key for PTY test host: {key:?}"
    ))
}

fn encode_key_without_mods(key: KeyCode) -> Result<Vec<u8>> {
    let bytes = match key {
        KeyCode::Backspace => b"\x7f".to_vec(),
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n) => {
            let code = tilde_key_code(KeyCode::F(n))
                .ok_or_else(|| anyhow!("unsupported function key F{n}"))?;
            format!("\x1b[{code}~").into_bytes()
        }
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Null => b"\0".to_vec(),
        KeyCode::Esc => b"\x1b".to_vec(),
        _ => {
            if let Some(codepoint) = csi_u_codepoint(key) {
                format!("\x1b[{codepoint}u").into_bytes()
            } else {
                return Err(anyhow!("unsupported key for PTY test host: {key:?}"));
            }
        }
    };
    Ok(bytes)
}

fn modifier_key_sequence_parts(key: KeyCode) -> Option<(&'static str, char)> {
    match key {
        KeyCode::Up => Some(("1", 'A')),
        KeyCode::Down => Some(("1", 'B')),
        KeyCode::Right => Some(("1", 'C')),
        KeyCode::Left => Some(("1", 'D')),
        KeyCode::End => Some(("1", 'F')),
        KeyCode::Home => Some(("1", 'H')),
        _ => None,
    }
}

fn tilde_key_code(key: KeyCode) -> Option<u8> {
    match key {
        KeyCode::Insert => Some(2),
        KeyCode::Delete => Some(3),
        KeyCode::PageUp => Some(5),
        KeyCode::PageDown => Some(6),
        KeyCode::F(1) => Some(11),
        KeyCode::F(2) => Some(12),
        KeyCode::F(3) => Some(13),
        KeyCode::F(4) => Some(14),
        KeyCode::F(5) => Some(15),
        KeyCode::F(6) => Some(17),
        KeyCode::F(7) => Some(18),
        KeyCode::F(8) => Some(19),
        KeyCode::F(9) => Some(20),
        KeyCode::F(10) => Some(21),
        KeyCode::F(11) => Some(23),
        KeyCode::F(12) => Some(24),
        _ => None,
    }
}

fn csi_u_codepoint(key: KeyCode) -> Option<u32> {
    match key {
        KeyCode::Char(c) => Some(c.into()),
        KeyCode::Null => Some(0),
        KeyCode::Enter => Some(13),
        KeyCode::Esc => Some(27),
        KeyCode::Backspace => Some(127),
        KeyCode::Tab => Some(9),
        KeyCode::CapsLock => Some(57358),
        KeyCode::ScrollLock => Some(57359),
        KeyCode::NumLock => Some(57360),
        KeyCode::PrintScreen => Some(57361),
        KeyCode::Pause => Some(57362),
        KeyCode::Menu => Some(57363),
        KeyCode::F(n @ 13..=35) => Some(57363 + u32::from(n)),
        KeyCode::KeypadBegin => Some(57427),
        _ => None,
    }
}

impl Drop for PtyTestHost {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(t) = self.read_thread.take() {
            let _ = t.join();
        }
    }
}
