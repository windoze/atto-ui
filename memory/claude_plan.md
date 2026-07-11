# Execution Plan

## Current task

- First incomplete TODO task: **M5.3 第 2 层 呈现【外壳层】**
- Requirement: Use component-layer `command_blocks()` data in the shell layer to render command block presentation: separators/background for command output and red markers for failed commands (`exit != 0`).
- Scope boundary: Complete only M5.3. Do not proceed to M5.4 interaction/navigation or later tasks.

## Steps

1. Inspect the command block snapshot API and existing terminal rendering/test fixtures.
2. Locate the outer shell/demo surfaces that own terminal windows and have access to `TerminalHandle::command_blocks()`.
3. Add a presentation mechanism that can render command block separators, output-region background, and failed-command red markers without changing behavior when no command blocks exist.
4. Add focused tests for semantic command block presentation, including success and failure blocks.
5. Run `cargo fmt`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and the relevant test suite.
6. Mark M5.3 `[DONE]` in `TODO.md` with completion and validation details.
7. Commit the M5.3 changes and stop.

## Progress

- Identified M5.3 as the first incomplete task.
- Confirmed the latest commit completed M5.2, which is the prerequisite for this task.
- Added opt-in OSC 133 command-block presentation support with separator glyphs, output-row background styling, and failed-command markers.
- Enabled command-block presentation in the terminal viewer and PTY shell fixture.
- Added targeted unit and PTY tests for command-block presentation.
- Completed formatting, focused tests, clippy, and the full workspace test suite successfully.
- Marked M5.3 as `[DONE]` in `TODO.md` with completion and validation details.
