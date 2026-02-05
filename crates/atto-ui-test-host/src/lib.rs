#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

pub struct PtyTestHost {
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
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

        let read_thread = thread::spawn(move || -> Result<()> {
            let mut buf = [0u8; 16 * 1024];
            loop {
                let n = reader.read(&mut buf).context("pty read")?;
                if n == 0 {
                    break;
                }
                let mut p = parser_for_thread
                    .lock()
                    .map_err(|_| anyhow!("parser poisoned"))?;
                p.process(&buf[..n]);
            }
            Ok(())
        });

        Ok(Self {
            child,
            writer,
            parser,
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
        let x1 = x.saturating_add(1);
        let y1 = y.saturating_add(1);
        let press = format!("\x1b[<0;{x1};{y1}M");
        let release = format!("\x1b[<0;{x1};{y1}m");
        self.send_str(&press)?;
        self.send_str(&release)?;
        Ok(())
    }

    /// Sends a mouse wheel scroll event using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn wheel_up(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_event(64, x, y)
    }

    /// Sends a mouse wheel scroll event with Shift held using xterm SGR mouse encoding.
    ///
    /// Many terminals map Shift+wheel to horizontal scroll; Crossterm also exposes this via the
    /// `modifiers` field on `MouseEvent`.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn wheel_up_shift(&mut self, x: u16, y: u16) -> Result<()> {
        // xterm encodes Shift as bit 2 (value 4) in the "button" parameter.
        self.wheel_event(64 + 4, x, y)
    }

    /// Sends a mouse wheel scroll event using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn wheel_down(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_event(65, x, y)
    }

    /// Sends a mouse wheel scroll event with Shift held using xterm SGR mouse encoding.
    ///
    /// Coordinates are **0-based** to match Crossterm's `MouseEvent` positions.
    pub fn wheel_down_shift(&mut self, x: u16, y: u16) -> Result<()> {
        // xterm encodes Shift as bit 2 (value 4) in the "button" parameter.
        self.wheel_event(65 + 4, x, y)
    }

    pub fn wheel_left(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_event(66, x, y)
    }

    pub fn wheel_right(&mut self, x: u16, y: u16) -> Result<()> {
        self.wheel_event(67, x, y)
    }

    fn wheel_event(&mut self, cb: u16, x: u16, y: u16) -> Result<()> {
        let x1 = x.saturating_add(1);
        let y1 = y.saturating_add(1);
        let seq = format!("\x1b[<{cb};{x1};{y1}M");
        self.send_str(&seq)
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

    pub fn screen_contents(&self) -> Result<String> {
        let p = self.parser.lock().map_err(|_| anyhow!("parser poisoned"))?;
        Ok(p.screen().contents().to_string())
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

impl Drop for PtyTestHost {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(t) = self.read_thread.take() {
            let _ = t.join();
        }
    }
}
