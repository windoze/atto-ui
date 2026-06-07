//! Opaque string handle maps for runtime ids exposed to JavaScript.

use std::collections::HashMap;
use std::marker::PhantomData;

use atto_ui::{CallbackId, WindowId};
use napi::Result;

use crate::error::invalid_arg;

/// Runtime id types that can be wrapped in JavaScript-facing handles.
pub trait HandleId: Copy {
    /// Return the numeric runtime id that must never be exposed directly to JavaScript.
    fn raw(self) -> u64;

    /// Rebuild the runtime id after resolving a handle.
    fn from_raw(raw: u64) -> Self;

    /// Human-readable id kind for diagnostics.
    fn kind() -> &'static str;

    /// Stable handle prefix for this id class.
    fn prefix() -> &'static str;
}

impl HandleId for CallbackId {
    fn raw(self) -> u64 {
        self.0
    }

    fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    fn kind() -> &'static str {
        "callback id"
    }

    fn prefix() -> &'static str {
        "callback"
    }
}

impl HandleId for WindowId {
    fn raw(self) -> u64 {
        self.raw()
    }

    fn from_raw(raw: u64) -> Self {
        Self::from_raw(raw)
    }

    fn kind() -> &'static str {
        "window id"
    }

    fn prefix() -> &'static str {
        "window"
    }
}

/// Bidirectional map between runtime ids and opaque JavaScript string handles.
#[derive(Debug)]
pub struct IdHandles<T: HandleId> {
    next_handle: u64,
    raw_to_handle: HashMap<u64, String>,
    handle_to_raw: HashMap<String, u64>,
    _marker: PhantomData<T>,
}

impl<T: HandleId> Default for IdHandles<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: HandleId> IdHandles<T> {
    /// Create an empty handle map for one runtime id class.
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            raw_to_handle: HashMap::new(),
            handle_to_raw: HashMap::new(),
            _marker: PhantomData,
        }
    }

    /// Return the existing handle for an id, or allocate a new opaque handle.
    pub fn handle_for(&mut self, id: T) -> String {
        let raw = id.raw();
        if let Some(handle) = self.raw_to_handle.get(&raw) {
            return handle.clone();
        }

        let handle = format!("atto:{}:{}", T::prefix(), self.next_handle);
        self.next_handle = self.next_handle.saturating_add(1);
        self.raw_to_handle.insert(raw, handle.clone());
        self.handle_to_raw.insert(handle.clone(), raw);
        handle
    }

    /// Resolve a JavaScript string handle back into a runtime id.
    pub fn resolve(&self, handle: &str) -> Result<T> {
        self.handle_to_raw
            .get(handle)
            .copied()
            .map(T::from_raw)
            .ok_or_else(|| invalid_arg(format!("unknown {} handle: {handle}", T::kind())))
    }

    /// Remove a runtime id and invalidate its JavaScript handle.
    pub fn release(&mut self, id: T) -> bool {
        let raw = id.raw();
        let Some(handle) = self.raw_to_handle.remove(&raw) else {
            return false;
        };
        self.handle_to_raw.remove(&handle);
        true
    }

    /// Resolve and remove a JavaScript handle in one step.
    pub fn release_handle(&mut self, handle: &str) -> Result<T> {
        let raw = self
            .handle_to_raw
            .remove(handle)
            .ok_or_else(|| invalid_arg(format!("unknown {} handle: {handle}", T::kind())))?;
        self.raw_to_handle.remove(&raw);
        Ok(T::from_raw(raw))
    }

    /// Return whether a JavaScript handle is currently valid.
    pub fn contains_handle(&self, handle: &str) -> bool {
        self.handle_to_raw.contains_key(handle)
    }

    /// Return whether a runtime id already has a JavaScript handle.
    pub fn contains_id(&self, id: T) -> bool {
        self.raw_to_handle.contains_key(&id.raw())
    }

    /// Return the number of live handles in this map.
    pub fn len(&self) -> usize {
        self.handle_to_raw.len()
    }

    /// Return whether the map has no live handles.
    pub fn is_empty(&self) -> bool {
        self.handle_to_raw.is_empty()
    }
}

/// Callback id handle map used by event bindings and callback invocations.
pub type CallbackHandles = IdHandles<CallbackId>;

/// Window id handle map used by AppHost window methods.
pub type WindowHandles = IdHandles<WindowId>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_handle_round_trips_without_exposing_raw_id() {
        let mut handles = CallbackHandles::new();
        let handle = handles.handle_for(CallbackId(42));

        assert_eq!(handles.resolve(&handle).unwrap(), CallbackId(42));
        assert_eq!(handles.handle_for(CallbackId(42)), handle);
        assert!(handle.starts_with("atto:callback:"));
        assert!(handle.parse::<u64>().is_err());
    }

    #[test]
    fn window_handles_use_an_independent_namespace() {
        let mut callbacks = CallbackHandles::new();
        let mut windows = WindowHandles::new();

        let callback = callbacks.handle_for(CallbackId(1));
        let window = windows.handle_for(WindowId::from_raw(1));

        assert_ne!(callback, window);
        assert_eq!(callbacks.resolve(&callback).unwrap(), CallbackId(1));
        assert_eq!(windows.resolve(&window).unwrap(), WindowId::from_raw(1));
    }

    #[test]
    fn released_handles_become_invalid() {
        let mut handles = CallbackHandles::new();
        let handle = handles.handle_for(CallbackId(7));

        assert!(handles.contains_handle(&handle));
        assert!(handles.release(CallbackId(7)));
        assert!(!handles.contains_handle(&handle));
        assert!(handles.resolve(&handle).is_err());
    }

    #[test]
    fn released_id_gets_new_handle_without_revalidating_stale_handle() {
        let mut handles = CallbackHandles::new();
        let stale = handles.handle_for(CallbackId(7));

        assert!(handles.release(CallbackId(7)));
        let current = handles.handle_for(CallbackId(7));

        assert_ne!(current, stale);
        assert!(handles.resolve(&stale).is_err());
        assert_eq!(handles.resolve(&current).unwrap(), CallbackId(7));
    }

    #[test]
    fn handles_are_rejected_across_namespaces() {
        let mut callbacks = CallbackHandles::new();
        let mut windows = WindowHandles::new();

        let callback = callbacks.handle_for(CallbackId(1));
        let window = windows.handle_for(WindowId::from_raw(1));

        assert!(callbacks.resolve(&window).is_err());
        assert!(windows.resolve(&callback).is_err());
    }

    #[test]
    fn release_handle_returns_runtime_id() {
        let mut handles = WindowHandles::new();
        let handle = handles.handle_for(WindowId::from_raw(9));

        assert_eq!(
            handles.release_handle(&handle).unwrap(),
            WindowId::from_raw(9)
        );
        assert!(handles.is_empty());
        assert!(handles.release_handle(&handle).is_err());
    }
}
