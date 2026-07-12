#![forbid(unsafe_code)]

//! Terminal emulator component.
//!
//! This crate provides [`TerminalEmulator`], a terminal widget that renders ANSI output
//! and forwards keyboard/mouse input to a consumer.

mod config;
mod dynamic;
mod pane;
mod selection;
mod session;
mod terminal;

pub use config::{
    DEFAULT_TERMINAL_PROFILE_NAME, DEFAULT_TERMINAL_SCROLL_STEP, DEFAULT_TERMINAL_SCROLLBACK_LEN,
    DEFAULT_TERMINAL_SHELL_FALLBACK, TerminalAlternateScreenScrollConfig, TerminalColorSpec,
    TerminalConfig, TerminalConfigFormat, TerminalCursorConfig, TerminalCursorShapeConfig,
    TerminalPaletteConfig, TerminalProfileConfig, TerminalSessionsConfig,
    TerminalShellIntegrationConfig, TerminalShortcutConfig, TerminalShortcutModifier,
};
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
