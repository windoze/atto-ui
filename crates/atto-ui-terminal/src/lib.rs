#![forbid(unsafe_code)]

//! Terminal emulator component.
//!
//! This crate provides [`TerminalEmulator`], a terminal widget that renders ANSI output
//! and forwards keyboard/mouse input to a consumer.

mod dynamic;
mod terminal;

pub use dynamic::{
    register_runtime_components, register_terminal_emulator, terminal_emulator_schema,
};
pub use terminal::{
    TerminalClipboardCopy, TerminalEmulator, TerminalHandle, TerminalShortcut, TerminalSnapshot,
};
