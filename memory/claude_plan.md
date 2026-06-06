# Claude Execution Plan

## Scope

Work through exactly one `TODO.md` task: the first task whose title is not prefixed with `[DONE]`. Stop after completing and committing that task, or after committing a required blocker/prerequisite update if completion is impossible.

## Plan

1. Read `TODO.md` and identify the first incomplete task by title prefix.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the files and tests relevant to the selected task.
4. Implement the task as specified, without narrowing scope or using workaround behavior.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task and repository policy.
6. If tests reveal unscheduled failures, fix them if in scope or add the minimum prerequisite task(s) to `TODO.md` before marking the current task complete.
7. Update `TODO.md` by prefixing the completed task title with `[DONE]` and adding a completion record with validation details.
8. Update this file whenever a key step completes or the plan changes.
9. Review `git status`, `git diff`, and recent commits, then commit all intended changes with a clear task-specific message.
10. Stop without starting the next task.

## Progress

- Initial execution plan recorded.
- Identified first incomplete task: `T11 — 可折叠 disclosure / accordion 组件（core）`.
- Latest commit `[R10] Review chat and terminal coverage` is not directly relevant to T11.
- Implementation approach: add a reusable core `Disclosure` widget with bound title/status/expanded/content state, optional child content, keyboard/mouse toggle handling, runtime registry exposure, and a PTY fixture covering T11 acceptance behavior.
- Implemented the Disclosure widget path and ran `cargo fmt` successfully.
- `cargo clippy --all-targets -- -D warnings` initially failed on Disclosure child context lifetime/borrow conflicts; fixing those before rerunning validation.
- Fixed the Disclosure child context borrowing issue; `cargo fmt` and `cargo clippy --all-targets -- -D warnings` now pass.
- Validation completed successfully: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --test pty_disclosure`, and `cargo test --workspace --all-targets` all pass.
- Marked T11 as `[DONE]` in `TODO.md`; no `PLAN.md` update needed because phase sequencing did not change.
