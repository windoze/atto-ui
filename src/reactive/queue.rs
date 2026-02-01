use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;

/// A simple multi-producer action queue for UI callbacks.
///
/// This is useful for wiring reactive callbacks (buttons, menu items, etc.) into a run loop
/// without relying on stringly-typed command dispatch.
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
