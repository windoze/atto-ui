//! Reactive state management primitives.
//!
//! This module provides a small foundation for building declarative/reactive layers on top of
//! Atto-UI's composable `Component` API.

mod dirty;
mod property;
mod queue;
mod timer;

pub use dirty::{DirtyFlag, DirtyObserver};
pub use property::{Binding, Property};
pub use queue::{EventQueue, drain_channel};
pub use timer::{
    TimerHandle, TimerWheel, cancel_timer, register_timer, register_timer_with_duration,
    set_global_tick_rate, tick_global_timers,
};
