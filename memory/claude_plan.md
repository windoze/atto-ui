# Claude Execution Plan

## Scope

- Source of truth: `TODO.md` determines the first incomplete task and its requirements.
- Current invocation goal: complete exactly the first incomplete task, validate it, update `TODO.md`, commit the result, then stop.
- Planning note: this file records the actionable plan, decisions, blockers, and progress updates. It does not include private chain-of-thought.

## Step-by-Step Plan

1. Read `TODO.md` first and identify the first task whose title is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Read the selected task details, dependencies, validation requirements, and completion-record expectations.
4. Inspect only the code and tests needed to understand and implement that task.
5. Implement the smallest correct change that fully satisfies the selected task, without workarounds or scope narrowing.
6. If a concrete blocker or prerequisite is discovered, update `TODO.md` with the minimum prerequisite task in dependency order, keep the current task incomplete, commit that bookkeeping, and stop.
7. Run validation in the required order after code changes: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full tests as required by the task.
8. If validation reveals unscheduled failures, fix them if in scope or add minimum prerequisite/follow-up tasks before marking the current task done.
9. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record with implementation and validation notes.
10. Inspect git status, diff, and recent log; stage only intended files; commit with a descriptive task-based message.
11. Stop after the commit and do not begin the next task.

## Progress Log

- Plan initialized before reading project task files or running commands.
- Selected first incomplete task from `TODO.md`: `M5.R Review`.
- Review scope: verify M5 layer separation, confirm command-level exit codes remain distinct from process-level exit status, run required validation, update `TODO.md`, commit, then stop.
- Review finding: implementation keeps OSC 133 command exit codes in `TerminalCommandBlock::exit_code`/`last_exit_code()` and process exits in `exit_status`/`on_exit`; no implementation change needed.
- Added a focused regression test proving OSC 133 `D` updates command-level status without setting process-level `exit_status()`.
- Validation passed: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted callbacks regression, and `cargo test --workspace --all-targets`.
- Updated `TODO.md` to mark `M5.R Review` as `[DONE]` with review and validation notes.
- Commit created: `[M5.R] Review terminal command markers`.
