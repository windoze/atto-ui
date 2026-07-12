#![forbid(unsafe_code)]

//! Terminal emulator component.
//!
//! This crate provides [`TerminalEmulator`], a terminal widget that renders ANSI output
//! and forwards keyboard/mouse input to a consumer.

mod dynamic;
mod pane;
mod selection;
mod session;
mod terminal;

pub use dynamic::{
    register_runtime_components, register_terminal_emulator, terminal_emulator_schema,
};
pub use pane::{
    TerminalPaneGroup, TerminalPaneGroupHandle, TerminalPaneId, TerminalPaneSnapshot,
    TerminalPaneSplit,
};
pub use selection::{TerminalSelectionPosition, TerminalSelectionRange};
pub use session::TerminalSessionSpec;
pub use terminal::{
    TerminalClipboardCopy, TerminalCommandBlock, TerminalCommandBlockPresentation,
    TerminalCursorShape, TerminalEmulator, TerminalHandle, TerminalPrefixBinding,
    TerminalPrefixCommand, TerminalShellIntegration, TerminalShortcut, TerminalSnapshot,
    TerminalSystemClipboard,
};
