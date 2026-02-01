//! Reactive state management primitives.
//!
//! This module provides a small foundation for building declarative/reactive layers on top of
//! Chatty's existing imperative `View` and `Control` APIs.

mod dirty;
mod observable;
mod property;

pub use dirty::DirtyFlag;
pub use observable::Observable;
pub use property::{Property, PropertyBinding};
