use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc;

use parking_lot::Mutex;

/// A simple multi-producer action queue for UI callbacks.
///
/// This is useful for wiring reactive callbacks (buttons, menu items, etc.) into a run loop
/// without relying on stringly-typed command dispatch.
///
/// ## Usage
///
/// For synchronous usage (existing code), use the VecDeque-based implementation:
/// ```rust
/// use atto_ui::reactive::EventQueue;
///
/// let queue = EventQueue::new();
/// queue.push(1u8);
/// for action in queue.drain() {
///     assert_eq!(action, 1u8);
/// }
/// ```
///
/// For asynchronous usage (background tasks), use the channel-based implementation:
/// ```rust,no_run
/// use std::sync::mpsc::RecvTimeoutError;
/// use std::time::Duration;
///
/// use atto_ui::reactive::EventQueue;
///
/// let (sender, receiver) = EventQueue::<String>::channel();
///
/// // Clone sender for background tasks
/// std::thread::spawn(move || {
///     sender.send("work_done".to_string()).ok();
/// });
///
/// // In main loop
/// match receiver.recv_timeout(Duration::from_millis(50)) {
///     Ok(action) => {
///         drop(action);
///     }
///     Err(RecvTimeoutError::Timeout) => { /* no action, continue */ }
///     Err(RecvTimeoutError::Disconnected) => {}
/// }
/// ```
#[derive(Clone, Debug)]
pub struct EventQueue<T> {
    inner: Arc<Mutex<VecDeque<T>>>,
}

impl<T> EventQueue<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Creates a channel-based event queue for async background tasks.
    ///
    /// Returns `(sender, receiver)` where:
    /// - `sender` can be cloned and sent to background threads
    /// - `receiver` should be used in the main event loop with `recv_timeout()`
    ///
    /// This is the recommended approach for handling background tasks that need to
    /// notify the main UI thread.
    pub fn channel() -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
        mpsc::channel()
    }

    pub fn push(&self, value: T) {
        self.inner.lock().push_back(value);
    }

    pub fn pop(&self) -> Option<T> {
        self.inner.lock().pop_front()
    }

    pub fn drain(&self) -> Vec<T> {
        let mut out = Vec::new();
        let mut guard = self.inner.lock();
        while let Some(v) = guard.pop_front() {
            out.push(v);
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

impl<T> Default for EventQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to drain all pending messages from a channel without blocking.
///
/// This is useful in the main event loop to process all queued actions at once.
pub fn drain_channel<T>(receiver: &mpsc::Receiver<T>) -> Vec<T> {
    let mut out = Vec::new();
    while let Ok(value) = receiver.try_recv() {
        out.push(value);
    }
    out
}
