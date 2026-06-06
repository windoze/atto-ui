#![forbid(unsafe_code)]

//! Tokio-backed helpers for integrating asynchronous work with `atto-ui`.
//!
//! The crate is feature-gated so the core `atto-ui` crate and default workspace builds do not pull
//! in tokio. Enable `tokio-runtime` for task spawning helpers, or `event-stream` for the async
//! crossterm run loop.

#[cfg(feature = "tokio-runtime")]
mod runtime;
#[cfg(feature = "event-stream")]
mod stream;

#[cfg(feature = "tokio-runtime")]
pub use runtime::{
    build_current_thread_runtime, build_multi_thread_runtime, spawn_async, spawn_blocking,
};

#[cfg(feature = "event-stream")]
pub use stream::{
    AsyncInput, next_terminal_event_or_action, run_crossterm_desktop_with_async_actions,
    run_crossterm_desktop_with_async_actions_and_tasks, terminal_event_stream,
};
