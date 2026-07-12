# Execution Plan

## Scope

- Follow `TODO.md` as the authoritative source.
- Complete exactly the first incomplete task, then stop after committing.
- Do not perform broad issue triage before selecting the current task.
- Update this file when the plan changes or when key milestones are completed.

## Initial Steps

1. Read `TODO.md` and identify the first task whose title is not prefixed with `[DONE]`.
2. Inspect the latest commit only for unfinished work directly relevant to that selected task.
3. Read the selected task details, dependencies, validation requirements, and completion-record format.

## Execution Steps

1. Implement the selected task as written, without narrowing scope or using workarounds.
2. If a concrete prerequisite blocks the task, add the minimum prerequisite task to `TODO.md`, keep the current task incomplete, update this file, commit, and stop.
3. Run validation in the required order when code changes are made: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full tests as required.
4. Address any unscheduled failing test by fixing it or scheduling the minimum prerequisite/follow-up in `TODO.md` before marking the task done.
5. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record.
6. Commit all task-related changes with a clear task-specific message.
7. Stop without starting the next task.

## Progress Log

- Plan initialized. Next step: read `TODO.md` to identify the first incomplete task.
- First incomplete task identified: `M6.2 会话管理`.
- Latest commit checked: `[M6.1] Add terminal split panes`; it directly precedes M6.2 but does not explicitly record an unfinished blocker.
- Current worktree before implementation only contains this plan file change.
- Next step: inspect M6 plan details and the existing terminal pane/session code paths.
- Implemented core M6.2 changes: added `TerminalSessionSpec`, `TerminalEmulator::spawn_session`, `TerminalHandle::current_cwd`, session-aware `terminal_viewer` new shell/command entries, session-aware snapshot fixture, and a PTY restart/cwd regression.
- Next step: run `cargo fmt`, then clippy and tests; fix any failures before marking `TODO.md` done.
- Validation completed successfully: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted session/PTV tests, and `cargo test --workspace --all-targets`.
- `TODO.md` updated with `[DONE] M6.2` and completion record.
- Next step: inspect final git diff/status, then commit the task changes and stop.
