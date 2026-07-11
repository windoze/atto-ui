# Execution Plan

## Current objective
Complete exactly the first incomplete task listed in `TODO.md`, then stop after marking it done and committing the changes.

Selected task: `M5.4 第 2 层 交互【外壳层】` — command-level navigation with `Ctrl+Up/Down`, command-output selection granularity, and right-click command actions for rerun/copy command/copy output.

## Steps
1. Read `TODO.md` to identify the first task whose heading is not prefixed with `[DONE]`.
2. Inspect only the files and context needed for that task, including `PLAN.md` only if the task or dependency structure requires it.
3. Implement the task fully, avoiding workarounds or unrelated changes.
4. Run the required validation in order: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task.
5. Fix any observed unscheduled failures or add the minimum prerequisite task to `TODO.md` if a blocker prevents completion.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling its completion record; update this plan file at major milestones.
7. Commit all changes for this task with a clear message and the required co-author trailer.

## Implementation notes for M5.4
1. [DONE] Add precise OSC 133 marker column tracking while keeping existing row-based command block fields available.
2. [DONE] Add `TerminalHandle` methods for command-block lookup, navigation, output selection, command/output copying, and rerun input.
3. [DONE] Wire the PTY shell fixture to handle `Ctrl+Up/Down` and right-click command context actions, exposing status text for deterministic tests.
4. [DONE] Apply the same command interaction wiring to the interactive terminal viewer example.
5. [DONE] Add targeted unit and PTY tests for command navigation, whole-output selection/copy, and right-click rerun/copy actions.
6. [DONE] Run formatting, linting, targeted tests, then the workspace test suite if code changed. Formatting, clippy, targeted terminal regressions, and the full workspace test suite have passed.
