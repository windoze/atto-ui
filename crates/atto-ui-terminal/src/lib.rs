#![forbid(unsafe_code)]

//! Terminal emulator component.
//!
//! This crate provides [`TerminalEmulator`], a terminal widget that renders ANSI output
//! and forwards keyboard/mouse input to a consumer.

mod config;
mod dynamic;
mod ipc;
mod pane;
mod selection;
mod session;
mod settings;
mod terminal;

pub use config::{
    DEFAULT_TERMINAL_PROFILE_NAME, DEFAULT_TERMINAL_SCROLL_STEP, DEFAULT_TERMINAL_SCROLLBACK_LEN,
    DEFAULT_TERMINAL_SHELL_FALLBACK, MAX_TERMINAL_SCROLLBACK_LEN,
    TerminalAlternateScreenScrollConfig, TerminalColorSpec, TerminalConfig, TerminalConfigFormat,
    TerminalCursorConfig, TerminalCursorShapeConfig, TerminalPaletteConfig, TerminalProfileConfig,
    TerminalSessionsConfig, TerminalShellIntegrationConfig, TerminalShortcutConfig,
    TerminalShortcutModifier, TerminalTmuxEnvironmentConfig,
};
pub use dynamic::{
    register_runtime_components, register_terminal_emulator, terminal_emulator_schema,
};
pub use ipc::{TerminalPaneIpc, terminal_pane_ipc_handler};
pub use pane::{
    TerminalPaneBreakOutcome, TerminalPaneGroup, TerminalPaneGroupHandle, TerminalPaneId,
    TerminalPaneSelectDirection, TerminalPaneSelectOutcome, TerminalPaneSnapshot,
    TerminalPaneSplit, TerminalPaneSplitOutcome,
};
pub use selection::{TerminalSelectionPosition, TerminalSelectionRange};
pub use session::TerminalSessionSpec;
pub use settings::{
    TerminalSettingsDraft, TerminalSettingsHandle, TerminalSettingsView,
    default_terminal_config_path, load_terminal_config_or_default,
};
pub use terminal::{
    TerminalClipboardCopy, TerminalCommandBlock, TerminalCommandBlockPresentation,
    TerminalCursorShape, TerminalEmulator, TerminalHandle, TerminalPrefixBinding,
    TerminalPrefixCommand, TerminalShellIntegration, TerminalShortcut, TerminalSnapshot,
    TerminalSystemClipboard,
};
