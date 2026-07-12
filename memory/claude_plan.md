# Execution Plan

## Scope
- Follow `TODO.md` as the authoritative ordered task list.
- Complete only the first task whose heading is not prefixed with `[DONE]`, then stop.
- Keep `PLAN.md` unchanged unless phase-level sequencing, dependencies, assumptions, or completion criteria change.

## Steps
1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Inspect the latest commit only for unfinished work directly relevant to that task.
3. Read the relevant implementation and tests for the selected task.
4. Implement the task with minimal, focused changes.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the required tests, using the full suite only when needed.
6. If unscheduled test failures or blocking spec mismatches appear, fix them or add the minimum prerequisite task in `TODO.md` and stop.
7. Mark the task title `[DONE]` in `TODO.md` and update its completion record.
8. Inspect git status/diff/log, commit all intended changes with a clear task-specific message, then stop.

## Previous Progress Log
- Created initial execution plan before project inspection.
- Identified first incomplete task: `M6.4 测试` in `TODO.md`.
- Current task scope: add PTY coverage for in-window split layout, dead session restart, and new shell/command creation landing in the specified cwd.
- Latest commit `[M6.3] Configure terminal spawn environment` has no directly relevant unfinished note.
- Existing PTY tests already cover split-pane layout and dead/restarted sessions; the remaining M6.4 gap is menu-driven new shell/command session creation with cwd inheritance/selection in the PTY fixture.
- Implemented fixture support for File menu `New shell window` / `New command window`, extra terminal sessions, focused-session cwd inheritance, and a status line exposing terminal count/focused profile/cwd.
- Added PTY coverage for creating command and shell windows from the File menu and verifying both subprocesses start in the OSC7-observed cwd.
- Targeted PTY test initially failed because the long temporary cwd wrapped inside the fixed-width terminal pane; adjust the test to use shorter temp paths rather than weakening the behavior check.
- Full test run found a fixture regression: adding a fifth status line hides the existing `FOCUS=... CAP=...` line at 80x24. Fix by keeping the original four status lines and appending new window status to the focus line.
- Added temp-dir cleanup to the new PTY test so reruns cannot inherit a stale counter from a failed prior run.
- Final verification passed after the cleanup change: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- --nocapture`, `cargo fmt --all -- --check`, and `cargo test --workspace --all-targets`.
- Next step: mark `M6.4 测试` as `[DONE]` in `TODO.md` with the completion record, then commit the task changes.

## Current Invocation Progress
- Plan file refreshed before code execution or repository commands.
- Selected first incomplete task: `M6.R Review`.
- Latest commit is `[M6.4] Add terminal session PTY coverage`; it does not mention unfinished work that changes this task.
- Inspection scope: `TerminalPaneGroup` split/focus/layout forwarding, terminal resize/spawn paths, `TerminalSessionSpec`, terminal viewer/snapshot shell session wiring, and existing M6 process/PTY tests.
- Review found no blocking issue requiring a prerequisite task or code change.
- Validation completed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` passed.
- `TODO.md` now marks `M6.R Review` as `[DONE]` with completion notes.

## Current Invocation Progress - M7.1
- Created/updated the execution plan file before inspecting project tasks.
- Identified first incomplete task as `M7.1 光标形状`.
- Latest commit is `[M6.R] Complete terminal split/session review`; it does not mention unfinished M7.1 work.
- Implemented DECSCUSR (`CSI Ps SP q`) cursor-shape tracking from vt100 unhandled CSI callbacks.
- Updated terminal rendering so block uses reverse video, underline uses underline styling, and bar uses a one-cell bar glyph instead of always reversing the cursor cell.
- Added targeted rendering coverage for block, underline, bar, and default cursor shape sequences.
- Validation completed: `cargo fmt --all`, targeted cursor-shape test, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` all passed.
- `TODO.md` now marks `M7.1 光标形状` as `[DONE]` with completion notes.
- Next step: commit the task changes.
