# Execution Plan

I will follow `TODO.md` as the authoritative task list and complete only the first task whose heading is not prefixed with `[DONE]`.

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit only for directly relevant unfinished work tied to that task.
3. Inspect the affected code and tests for the selected task.
4. Implement the task completely, avoiding unrelated changes and avoiding workarounds.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite required by the task.
6. If tests expose unscheduled failures, fix them or add the minimum prerequisite task before marking the current task complete.
7. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling in its completion record.
8. Update this progress file at key milestones.
9. Commit all task-related changes with a descriptive message and the required co-author trailer.
10. Stop after the commit without starting the next task.

## Progress

- Plan file refreshed.
- Selected first incomplete task: `M3.5 测试` in `TODO.md`.
- Inspection found existing PTY coverage for `prefix+F10`, `prefix+w`, and `prefix+z`; remaining coverage is `prefix+prefix` to a child process, non-terminal global shortcut routing, and capture-release global F10 routing.
- Added PTY tests for literal prefix forwarding and direct global shortcut routing from both non-terminal focus and released terminal capture.
- Validation completed: targeted terminal PTY tests, workspace formatting check, clippy with warnings denied, and full workspace tests all passed.
- `TODO.md` updated to mark `M3.5` as `[DONE]`.
# Execution Plan (2026-07-12 invocation)

I cannot record private chain-of-thought, but this file tracks the concrete execution plan and progress for this invocation.

1. Read `TODO.md` and identify the first task whose title is not prefixed with `[DONE]`.
2. Check the latest commit message only for directly relevant unfinished work tied to that selected task.
3. Inspect the code and tests needed for that task without doing broad unrelated triage.
4. Implement the selected task completely, preserving repository conventions and avoiding workarounds.
5. Run `cargo fmt`, then targeted validation as needed, then `cargo clippy --all-targets -- -D warnings`, and finally the full test suite if compiled-output changes were made.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and adding a completion record with validation details.
7. Update this file at key milestones.
8. Commit all changes for the completed task with a descriptive message and the required co-author trailer.

## Progress

- Started invocation and refreshed the execution plan before project inspection.
- Selected first incomplete task: `M3.R Review` in `TODO.md`.
- Scope for this invocation: review M3 prefix-key behavior for reliable subprocess escape forwarding, configurable command handling, and double-prefix escape behavior; fix only directly blocking issues if found.
- Review finding: prefix fallback and double-prefix escape are covered, but the prefix command table is still hardcoded. I will add a small configurable binding API with default bindings preserved, add focused tests for remapping/replacing bindings, then rerun the required validation.
- Implemented configurable prefix command bindings on `TerminalEmulator` and `TerminalHandle`; defaults remain `F10`, `w`, `z`, `[`, and `prefix+prefix` still bypasses the table to send one literal prefix.
- Targeted validation passed for terminal prefix unit tests and M3 PTY prefix interactions. Next: workspace fmt check, clippy, and full workspace tests.
- Workspace validation passed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Marked `M3.R Review` as `[DONE]` in `TODO.md` with completion and validation notes.
- After tightening the replacement test, reran targeted prefix tests, targeted M3 PTY checks, formatting check, clippy, and full workspace tests successfully.

# Execution Plan (2026-07-12 M4.1 invocation)

I cannot record private chain-of-thought, but this file tracks the concrete execution plan and progress for this invocation.

1. Use `TODO.md` as the source of truth and complete only the first incomplete task: `M4.1 selection 状态机`.
2. Check the latest commit message for directly relevant unfinished M4.1 work.
3. Inspect the terminal component, existing text-selection implementation in the chat component, and terminal tests to understand current selection/copy-mode scaffolding.
4. Add a unified terminal selection state machine that supports selection ranges, highlighting, hit testing, and extracting selected text from the vt100 screen, with shared keyboard/mouse-facing helpers.
5. Add focused tests for selection range normalization, wide-character extraction where feasible, highlight behavior, and handle/query APIs needed by later M4 tasks.
6. Run `cargo fmt`, targeted terminal tests, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
7. Mark `M4.1` as `[DONE]` in `TODO.md` with a completion record and validation details.
8. Commit all changes for this invocation and stop.

## Progress

- Selected first incomplete task: `M4.1 selection 状态机`.
- Implemented a focused terminal selection module with absolute scrollback/screen positions, normalized ranges, visible-cell hit testing, wide-character-aware highlight ranges, and selected-text extraction from the vt100 screen.
- Wired selection state into `TerminalShared`, terminal drawing, and `TerminalHandle` APIs (`begin_selection`, `update_selection`, `clear_selection`, `selection_range`, `selection_position_for_view_cell`, `selected_text`).
- Added focused unit/integration tests for reversed range normalization, scrollback hit testing, wide-character range expansion, selected text extraction, and visible selection highlighting. Targeted selection tests pass.
- Required validation passed: `cargo fmt --all`, targeted terminal selection tests, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Marked `M4.1 selection 状态机` as `[DONE]` in `TODO.md` with completion and validation notes.
