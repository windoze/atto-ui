//! PTY lifecycle: child-process handle, resize, exit recording, input
//! forwarding, and callback dispatch helpers.

use super::*;

pub(crate) type PtyChild = Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>;

#[derive(Clone)]
pub(crate) struct TerminalPtyResize {
    pub(crate) master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    pub(crate) last_size: Arc<Mutex<(u16, u16)>>,
}

impl TerminalPtyResize {
    pub(crate) fn new(master: Box<dyn portable_pty::MasterPty + Send>, rows: u16, cols: u16) -> Self {
        Self {
            master: Arc::new(Mutex::new(master)),
            last_size: Arc::new(Mutex::new((rows, cols))),
        }
    }

    pub(crate) fn resize_if_needed(&self, rows: u16, cols: u16) -> bool {
        let mut last_size = self.last_size.lock();
        if *last_size == (rows, cols) {
            return false;
        }
        let _ = self.master.lock().resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        *last_size = (rows, cols);
        true
    }
}

pub(crate) struct TerminalProcess {
    pub(crate) _pty_resize: TerminalPtyResize,
    pub(crate) child: PtyChild,
    pub(crate) reader_alive: Arc<AtomicBool>,
    pub(crate) reader_thread: Option<thread::JoinHandle<()>>,
    pub(crate) exit_watcher_thread: Option<thread::JoinHandle<()>>,
    pub(crate) _shell_integration_files: Option<TerminalShellIntegrationFiles>,
}

impl TerminalProcess {
    pub(crate) fn record_exit_if_ready(&mut self, shared: &Arc<Mutex<TerminalShared>>) -> bool {
        try_record_child_exit(shared, &self.child)
    }

    pub(crate) fn shutdown(&mut self, shared: &Arc<Mutex<TerminalShared>>) {
        let already_exited = self.record_exit_if_ready(shared);
        self.reader_alive.store(false, Ordering::Relaxed);
        if !already_exited {
            // Signal the child, then reap it. `kill()` alone leaves a zombie
            // because we just disabled the reader/exit-watcher threads (the only
            // code paths that call `try_wait`), and `Box<dyn Child>`'s Drop does
            // not `wait()`. Block until the process is reaped and record the
            // resulting status so `on_exit` still fires.
            let status = {
                let mut child = self.child.lock();
                let _ = child.kill();
                child.wait().ok()
            };
            if let Some(status) = status {
                record_exit_status(shared, status);
            }
        }
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.exit_watcher_thread.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) fn try_record_child_exit(shared: &Arc<Mutex<TerminalShared>>, child: &PtyChild) -> bool {
    let status = match child.lock().try_wait() {
        Ok(Some(status)) => status,
        Ok(None) | Err(_) => return false,
    };
    record_exit_status(shared, status);
    true
}

pub(crate) fn record_exit_status(shared: &Arc<Mutex<TerminalShared>>, status: ExitStatus) {
    let callback = {
        let mut shared = shared.lock();
        if shared.exit_status.is_some() {
            return;
        }
        shared.exit_status = Some(status.clone());
        shared.process_running = false;
        shared.input_forward = None;
        shared.pty_resize = None;
        shared.on_exit.clone()
    };

    if let Some(callback) = callback {
        callback(status);
    }
}

pub(crate) fn dispatch_input(shared: &Arc<Mutex<TerminalShared>>, bytes: &[u8]) {
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

pub(crate) fn forward_input(shared: &Arc<Mutex<TerminalShared>>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let cb = { shared.lock().input_forward.clone() };
    if let Some(cb) = cb {
        cb(bytes);
    }
}

pub(crate) fn resize_terminal(shared: &Arc<Mutex<TerminalShared>>, rows: u16, cols: u16) -> bool {
    if rows == 0 || cols == 0 {
        return false;
    }
    let (screen_changed, pty_resize) = {
        let mut shared = shared.lock();
        let screen_changed = shared.resize_screen(rows, cols);
        (screen_changed, shared.pty_resize.clone())
    };
    let pty_changed = pty_resize
        .map(|resize| resize.resize_if_needed(rows, cols))
        .unwrap_or(false);
    screen_changed || pty_changed
}

pub(crate) fn dispatch_system_clipboard_copy(shared: &Arc<Mutex<TerminalShared>>, text: &str) {
    let clipboard = { shared.lock().system_clipboard.clone() };
    let Some(clipboard) = clipboard else {
        return;
    };
    let result = clipboard.copy_text(text);
    let mut shared = shared.lock();
    shared.last_system_clipboard_text = Some(text.to_string());
    shared.last_system_clipboard_error = result.err().map(|error| error.to_string());
}

pub(crate) fn dispatch_terminal_callback_events(
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
