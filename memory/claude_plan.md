# Execution Plan

## Current objective
Complete exactly the first incomplete task listed in `TODO.md`, then stop after documenting completion and committing the changes.

## Constraints and assumptions
- `TODO.md` is the authoritative task list. A task is complete only if its heading is prefixed with `[DONE]`.
- I will not proceed to later tasks after completing the first incomplete task.
- I will avoid broad historical triage before selecting the current task.
- If the selected task is blocked by a concrete prerequisite, I will add the minimum prerequisite task to `TODO.md`, commit that bookkeeping change, and stop.
- I will update `PLAN.md` only if phase-level sequencing, assumptions, dependencies, or completion criteria change.
- I will use formatting, linting, and tests required by the task and repository policy before marking completion.

## Step-by-step plan
1. Read `TODO.md` to identify the first incomplete task by heading order.
2. Inspect the latest commit only for directly relevant unfinished work if needed for that selected task.
3. Read the selected task details, dependencies, and validation requirements.
4. Inspect the relevant code and tests for the selected task.
5. Implement the task completely, avoiding unrelated changes and workarounds.
6. Add or update focused tests for the changed behavior.
7. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the required test command(s), escalating to the full suite if required.
8. If an unscheduled test failure appears, fix it if in scope or add the minimum prerequisite/follow-up task in `TODO.md` before marking completion.
9. Mark the completed task heading with `[DONE]` and update its completion record in `TODO.md`.
10. Commit all task-related changes with a descriptive message and the required co-author trailer.
11. Stop without beginning the next task.

## Progress log
- Prior progress from previous invocations:
  - Created this execution plan before reading or modifying project files.
  - Identified first incomplete task: `M2.1 死窗口回收`.
  - Implemented shell-level terminal session tracking in the terminal viewer and PTY fixture: exited child processes release capture, show the configured exit prompt, and restart the focused dead terminal when plain `R` is pressed.
  - Added a PTY regression that launches a child shell, verifies the exit prompt/status, presses `R`, and observes the restart counter.
  - Validation completed successfully for M2.1, and `TODO.md` was updated to mark only `M2.1 死窗口回收` as `[DONE]`.
  - Identified first incomplete task: `M2.2 标题联动`.
  - Implemented UI-thread polling of terminal OSC titles in `terminal_viewer` and the snapshot terminal window fixture, reset default titles on restart, refreshed the Windows menu from current `Window.title`, and added a PTY regression for titlebar/menu propagation.
  - Validation completed successfully for M2.2, and `TODO.md` was updated to mark only `M2.2 标题联动` as `[DONE]`.
  - Selected first incomplete task: `M2.3 测试` in `TODO.md`.
  - Confirmed existing PTY coverage for dead process prompt/restart and OSC 2 title linkage; added OSC 0 title linkage coverage.
  - Added OSC 0/2 PTY title-linkage coverage, completed validation, and marked `M2.3` done in `TODO.md`.
- Current invocation:
  - Refreshed this execution plan before task execution.
  - Identified first incomplete task: `M3.1 前缀态状态机`.
  - Latest commit is `[M2.R] Review terminal lifecycle and title sync`; it does not mention unfinished work directly blocking M3.1.
  - Implementation plan for M3.1: add a default component-level prefix shortcut state to `TerminalShared`, make captured prefix presses enter a pending state without forwarding, make the next non-release key consult an initially empty prefix command hook, and fallback-forward the held prefix plus the next encoded key so subprocess input is not lost.
  - Add focused component-level tests for held-prefix no-forward behavior, fallback forwarding, release-event handling while pending, and capture release clearing pending prefix state.
  - Implemented the prefix-pending state for captured key handling, including Tab/BackTab capture-hook routing, and added focused `input_encoding` tests.
  - Validation passed: `cargo fmt --all`, `cargo test -p atto-ui-terminal --test input_encoding terminal_prefix -- --nocapture`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
  - Marked `M3.1 前缀态状态机` as `[DONE]` in `TODO.md` with the completion record.
