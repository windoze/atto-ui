#![forbid(unsafe_code)]

//! Terminal emulator component.
//!
//! This crate provides [`TerminalEmulator`], a terminal widget that renders ANSI output
//! and forwards keyboard/mouse input to a consumer.

mod terminal;

pub use terminal::{TerminalEmulator, TerminalHandle, TerminalShortcut, TerminalSnapshot};
