//! OSC 133/7 command-block model ([`TerminalCommandBlock`]) plus the prefix
//! command / copy-mode state types.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPrefixCommand {
    ActivateMenu,
    ToggleWindowManagement,
    ToggleMaximize,
    EnterCopyMode,
    PasteCopyBuffer,
    SendPrefix,
}

/// One configurable binding in the prefix command table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalPrefixBinding {
    pub shortcut: TerminalShortcut,
    pub command: TerminalPrefixCommand,
}

impl TerminalPrefixBinding {
    pub fn new(shortcut: TerminalShortcut, command: TerminalPrefixCommand) -> Self {
        Self {
            shortcut: normalize_prefix_binding_shortcut(shortcut),
            command,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalCopyModeState {
    pub(crate) cursor: TerminalSelectionPosition,
    pub(crate) selecting: bool,
}

impl TerminalCopyModeState {
    pub(crate) fn new(cursor: TerminalSelectionPosition) -> Self {
        Self {
            cursor,
            selecting: false,
        }
    }
}

/// OSC 133/7 command block markers observed in terminal output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalCommandBlock {
    /// Absolute terminal row where the prompt started.
    pub prompt_start: Option<usize>,
    /// Terminal column where the prompt-start marker was observed.
    pub prompt_start_col: Option<u16>,
    /// Absolute terminal row where the command text started.
    pub command_start: Option<usize>,
    /// Terminal column where the command-start marker was observed.
    pub command_start_col: Option<u16>,
    /// Absolute terminal row where command output started.
    pub output_start: Option<usize>,
    /// Terminal column where the output-start marker was observed.
    pub output_start_col: Option<u16>,
    /// Absolute terminal row where the command finished.
    pub end: Option<usize>,
    /// Terminal column where the command-finished marker was observed.
    pub end_col: Option<u16>,
    /// Command-level exit code reported by OSC 133 `D`, if present.
    pub exit_code: Option<i32>,
    /// Current working directory reported by OSC 7 for this block.
    pub cwd: Option<String>,
}

impl TerminalCommandBlock {
    pub(crate) fn at_prompt(row: usize, col: u16, cwd: Option<String>) -> Self {
        Self {
            prompt_start: Some(row),
            prompt_start_col: Some(col),
            cwd,
            ..Self::default()
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.end.is_none()
    }

    pub(crate) fn has_command_activity(&self) -> bool {
        self.command_start.is_some() || self.output_start.is_some()
    }

    pub(crate) fn anchor_row(&self) -> Option<usize> {
        self.prompt_start
            .or(self.command_start)
            .or(self.output_start)
            .or(self.end)
    }

    pub(crate) fn last_row(&self) -> Option<usize> {
        [
            self.prompt_start,
            self.command_start,
            self.output_start,
            self.end,
        ]
        .into_iter()
        .flatten()
        .max()
    }

    pub(crate) fn contains_row(&self, row: usize) -> bool {
        let Some(start) = self.anchor_row() else {
            return false;
        };
        let end = self.last_row().unwrap_or(start);
        row >= start && row <= end
    }
}
