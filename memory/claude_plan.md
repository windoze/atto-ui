# Claude Execution Plan

## Scope

- Follow `TODO.md` as the authoritative task list.
- Complete exactly the first incomplete task whose heading is not prefixed with `[DONE]`.
- Stop after committing that task, or after committing any required prerequisite/blocker update.

## Steps

1. Read `TODO.md` and identify the first incomplete task.
2. Check the latest commit message only for unfinished work directly relevant to that task.
3. Inspect the code and tests needed for the selected task.
4. Implement the task without narrowing scope or introducing workarounds.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full tests required by the task.
6. If tests fail and the failure is not already explicitly scheduled, fix it or add the minimum prerequisite task before marking the task done.
7. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling its completion record.
8. Update `PLAN.md` only if phase-level sequencing or completion criteria changed.
9. Commit all intended changes with a clear task-specific commit message.
10. Stop without starting the next task.

## Progress Log

- Planned execution workflow before reading project task details.
- Identified first incomplete task: `P6.2 权限层级渲染` in `TODO.md`.
- Current task goal: update `crates/atto-ui-chat/src/list.rs` approval rendering so hierarchical options such as allow once, always allow, project allow, and deny are visible; after selection the approval area must be locked and show the selected level.
- Next steps: inspect latest commit for directly relevant unfinished notes, inspect approval model/store/list tests, implement minimal rendering changes, add/update tests, run formatting/lint/tests, update `TODO.md`, then commit and stop.
- Implemented `list.rs` approval label helpers so unresolved buttons and resolved labels render structured action/level scope. Added unit coverage for once/always/project/deny labels and the locked view rendering path.
- Full test run exposed a regression in `chat_inline_approval_buttons_emit_and_lock`: appending scope to already descriptive labels made the horizontal approval row too wide for the existing snapshot viewport, so no buttons were visible. Adjusting the helper to avoid duplicate scope when the label already names the selected level/action.
- Final validation passed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p atto-ui-chat approval --lib`, exact inline approval PTY test, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets`, and `cargo test --all --all-targets`.
- Marked `P6.2` as `[DONE]` in `TODO.md` with completion notes and the resolved test-failure note. Next step is to commit only this task's changes.
