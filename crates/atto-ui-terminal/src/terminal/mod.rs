//! Terminal emulator internals.
//!
//! This module collects the terminal emulator implementation. The public surface
//! ([`TerminalEmulator`], [`TerminalHandle`], [`TerminalSnapshot`], palette / shortcut /
//! command-block / shell-integration types, OSC-52 clipboard) is re-exported from
//! here for [`crate`]; the supporting machinery is split into focused submodules:
//!
//! - `config_types`: resolved runtime config, palette, shortcut, cursor shape.
//! - `clipboard` / `tmux_dcs`: OSC 52 system clipboard and tmux DCS passthrough.
//! - `command_block` / `command_blocks_render`: OSC 133/7 command-block model + rendering.
//! - `callbacks`: vt100 parser callback glue, OSC parsing, paste encoding.
//! - `shell_integration`: OSC 133/7 startup snippets + spawn preparation.
//! - `shared`: `TerminalShared` shared state and its methods.
//! - `captured_key` / `dsr`: capture-mode key routing and DSR query-reply handling.
//! - `pty`: PTY child / resize / exit / input-forwarding lifecycle.
//! - `emulator`: the `TerminalEmulator` widget and its trait impls.
//! - `handle`: `TerminalHandle` accessor API and `TerminalSnapshot`.
//! - `encode`: rendering + key/mouse escape-sequence encoding helpers.

use std::collections::VecDeque;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow, bail, ensure};
use base64::Engine;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, ExitStatus, PtySize, native_pty_system};
use ratatui::Frame;
use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use atto_ui::composable::{
    Capture, ComponentAction, ComponentContext, EventOutcome, EventResult, MouseCoordinateSpace,
    ScrollConfig,
};
use atto_ui::theme::Theme;

use crate::selection::{
    TerminalSelectionPosition, TerminalSelectionRange, TerminalSelectionState,
    position_for_view_cell, selected_cell_ranges_for_screen_row, selected_text_from_screen,
    visible_top_row,
};
use crate::session::TerminalSessionSpec;
use crate::{
    TerminalAlternateScreenScrollConfig, TerminalConfig, TerminalPaletteConfig,
    TerminalTmuxEnvironmentConfig,
};

const DEFAULT_TERM_ENV: &str = "xterm-256color";
const DEFAULT_COLORTERM_ENV: &str = "truecolor";
const TMUX_TERM_ENV: &str = "tmux-256color";
const COMMAND_SEPARATOR_SYMBOL: &str = "─";
const COMMAND_FAILURE_SYMBOL: &str = "!";
const CURSOR_BAR_SYMBOL: &str = "▏";
static SHELL_INTEGRATION_TEMP_ID: AtomicU64 = AtomicU64::new(0);

mod callbacks;
mod captured_key;
mod clipboard;
mod command_block;
mod command_blocks_render;
mod config_types;
mod dsr;
mod emulator;
mod encode;
mod handle;
mod pty;
mod shared;
mod shell_integration;
mod tmux_dcs;

// Flatten the submodule items into the parent scope so cross-module references
// (e.g. `TerminalShared`, `encode_key_event`, `dispatch_input`) resolve just as
// they did when everything lived in one file. The submodules declare their
// top-level items `pub(crate)` for this purpose; the public types stay `pub`,
// and `pub use … *` here re-exports them with their original visibility so
// `lib.rs` can keep its existing `pub use terminal::{…}` list.
pub use clipboard::*;
pub use command_block::*;
pub use config_types::*;
pub use emulator::*;
pub use handle::*;

// Internal-only submodules (no public types): plain globs to keep cross-module
// references resolving without leaking internal symbols via `pub use`.
use callbacks::*;
use captured_key::*;
use command_blocks_render::*;
use dsr::*;
use encode::*;
use pty::*;
use shared::*;
use shell_integration::*;
use tmux_dcs::*;

#[cfg(test)]
mod tests;

fn default_prefix_bindings() -> Vec<TerminalPrefixBinding> {
    vec![
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::F(10), KeyModifiers::NONE),
            TerminalPrefixCommand::ActivateMenu,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('w'), KeyModifiers::NONE),
            TerminalPrefixCommand::ToggleWindowManagement,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('z'), KeyModifiers::NONE),
            TerminalPrefixCommand::ToggleMaximize,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('['), KeyModifiers::NONE),
            TerminalPrefixCommand::EnterCopyMode,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char(']'), KeyModifiers::NONE),
            TerminalPrefixCommand::PasteCopyBuffer,
        ),
    ]
}

fn prefix_shortcut_from_letter(letter: char) -> Result<TerminalShortcut> {
    normalize_prefix_shortcut(TerminalShortcut::new(
        KeyCode::Char(letter),
        KeyModifiers::CONTROL,
    ))
}

fn normalize_prefix_shortcut(shortcut: TerminalShortcut) -> Result<TerminalShortcut> {
    ensure!(
        shortcut.modifiers == KeyModifiers::CONTROL,
        "terminal prefix shortcut must be plain Ctrl+<ASCII letter>"
    );
    let KeyCode::Char(letter) = shortcut.code else {
        bail!("terminal prefix shortcut must be plain Ctrl+<ASCII letter>");
    };
    ensure!(
        letter.is_ascii_alphabetic(),
        "terminal prefix shortcut must be plain Ctrl+<ASCII letter>"
    );
    Ok(TerminalShortcut {
        code: KeyCode::Char(letter.to_ascii_lowercase()),
        modifiers: KeyModifiers::CONTROL,
    })
}

fn normalize_prefix_binding_shortcut(shortcut: TerminalShortcut) -> TerminalShortcut {
    let code = match shortcut.code {
        KeyCode::Char(letter) => KeyCode::Char(letter.to_ascii_lowercase()),
        code => code,
    };
    TerminalShortcut {
        code,
        modifiers: shortcut.modifiers,
    }
}

type InputCallback = Arc<dyn Fn(&[u8]) + Send + Sync>;
type ExitCallback = Arc<dyn Fn(ExitStatus) + Send + Sync>;
type TextCallback = Arc<dyn Fn(&str) + Send + Sync>;
type BellCallback = Arc<dyn Fn() + Send + Sync>;
type ClipboardCopyCallback = Arc<dyn Fn(&TerminalClipboardCopy) + Send + Sync>;
type CommandFinishedCallback = Arc<dyn Fn(&TerminalCommandBlock) + Send + Sync>;
type SystemClipboard = Arc<dyn TerminalSystemClipboard>;
type TerminalParser = vt100::Parser<TerminalCallbacks>;
const TMUX_DCS_PREFIX: &[u8] = b"tmux;";
const TMUX_DCS_MAX_BUFFERED: usize = 1024 * 1024;
/// Upper bound on the partial-CSI tail buffered by [`collect_dsr_responses`].
///
/// A real query (DSR / Device Attributes / kitty flags) is only a handful of
/// bytes, so anything longer is not a query we will ever complete. Capping the
/// buffer prevents a program that emits `ESC [` followed by an unbounded run of
/// digits (with no final byte) from driving unbounded memory growth.
const DSR_TAIL_MAX: usize = 64;
