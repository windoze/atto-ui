//! Command-block rendering helpers: per-row presentation, separator / failure
//! styling, and block-text trimming.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandRowPresentation {
    pub(crate) separator: bool,
    pub(crate) output: bool,
    pub(crate) failed_marker: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum CommandBlockTextKind {
    Command,
    Output,
}

pub(crate) fn command_row_presentation(blocks: &[TerminalCommandBlock], row: usize) -> CommandRowPresentation {
    let mut presentation = CommandRowPresentation::default();
    for block in blocks {
        if block.prompt_start == Some(row) {
            presentation.separator = true;
        }

        if let Some(output_start) = block.output_start {
            let output_end = block.end.unwrap_or(usize::MAX);
            if row >= output_start && row < output_end {
                presentation.output = true;
            }
        }

        if block.exit_code.is_some_and(|code| code != 0)
            && block
                .end
                .or(block.output_start)
                .or(block.command_start)
                .or(block.prompt_start)
                == Some(row)
        {
            presentation.failed_marker = true;
        }
    }
    presentation
}

/// Returns whether a visible screen row has no drawable content, used to detect
/// rows blanked by an in-place erase (see `prune_cleared_command_marks`).
pub(crate) fn command_mark_row_is_blank(screen: &vt100::Screen, row: u16, width: u16) -> bool {
    (0..width).all(|x| {
        screen
            .cell(row, x)
            .is_none_or(|cell| cell.is_wide_continuation() || cell.contents().is_empty())
    })
}

pub(crate) fn command_separator_start(screen: &vt100::Screen, row: u16, width: u16) -> u16 {
    let mut content_end = 0;
    for x in 0..width {
        let Some(cell) = screen.cell(row, x) else {
            continue;
        };
        if cell.is_wide_continuation() || cell.contents().is_empty() {
            continue;
        }
        content_end = x.saturating_add(1);
    }
    if content_end == 0 {
        0
    } else {
        content_end.saturating_add(1).min(width)
    }
}

pub(crate) fn command_output_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-output")
        .unwrap_or_else(|| {
            let mut style = Style::default();
            if let Some(bg) = theme.status_bar.bg {
                style = style.bg(bg);
            }
            style
        })
}

pub(crate) fn command_separator_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-separator")
        .unwrap_or_else(|| theme.status_bar_key.add_modifier(Modifier::BOLD))
}

pub(crate) fn command_failure_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-failure")
        .or_else(|| theme.named_style("status-segment-error"))
        .unwrap_or_else(|| Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
}

pub(crate) fn trim_terminal_block_text(mut text: String) -> Option<String> {
    while text.ends_with('\n') || text.ends_with('\r') {
        text.pop();
    }
    (!text.is_empty()).then_some(text)
}
