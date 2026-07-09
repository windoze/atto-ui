# Execution Plan

## Scope

- Read `TODO.md` first and identify the first task whose title is not prefixed with `[DONE]`.
- Treat that task as the only implementation target for this invocation.
- Check the latest commit only for unfinished work directly relevant to that selected task.
- Avoid broad historical triage unless required by the selected task or by test failures encountered during validation.

## Steps

1. Inspect `TODO.md` and locate the first incomplete task.
2. Review the selected task body, dependencies, validation requirements, and completion-record format.
3. Inspect the minimal relevant code and recent commit context needed for that task.
4. Implement the task directly, unless a concrete prerequisite blocker makes correct implementation impossible.
5. If a blocker is found, update `TODO.md` with the minimum prerequisite task before the blocked task, commit that bookkeeping, and stop.
6. Run formatting first, then clippy with warnings denied, then the required/full test suite as appropriate.
7. Fix any observed unscheduled test failures or schedule them before marking the task complete.
8. Mark the task title `[DONE]` in `TODO.md` and update its completion record with implementation and validation notes.
9. Update `PLAN.md` only if phase-level sequencing or completion criteria changed.
10. Commit all changes relevant to the completed task with a descriptive message.
11. Stop after exactly one task.

## Progress Log

- Plan initialized before repository inspection.
- Selected first incomplete task: `P6.5 快照与测试`.
- Recent commit history shows P6.4 was completed immediately before this task; no latest-commit unfinished note was found in the commit subject.
- Current worktree change before implementation is this plan file only.
- Implemented a dedicated `--p6-approval-compact` snapshot fixture and PTY coverage for project-scope approval locking and compact block rendering.
- Targeted PTY first run showed the tool title/input were above the visible viewport while the approval UI was visible; adjusted the approval test to assert the visible prompt, options, event payload, and locked state.
- Targeted PTY second run showed the approval event wraps in the fixed-width window and auto-follow moves the locked approval row offscreen; adjusted the test to assert wrapped event fields and scroll back before checking the locked row.
- `cargo test -p atto-ui-chat --test pty_chat chat_p6 -- --nocapture` now passes for the two new P6 PTY cases.
- Validation passed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets`, and `cargo test --all --all-targets`.
- Updated `TODO.md` to mark P6.5 as `[DONE]` with implementation, test, and validation notes. No `PLAN.md` update was needed because phase-level sequencing did not change.
