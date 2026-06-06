# Claude Execution Plan

## Scope

- Follow `TODO.md` as the authoritative task list.
- Select the first task whose title is not prefixed with `[DONE]`.
- Complete exactly that task, then stop after committing the result.

## Plan

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit only for directly relevant unfinished context.
3. Inspect the smallest relevant part of the codebase for the selected task.
4. Implement the task without workarounds or scope narrowing.
5. Run formatting, linting, and relevant tests; run the full suite when required.
6. Update `TODO.md` completion status and record validation results.
7. Commit all intended changes with a task-specific message.
8. Stop without starting the next task.

## Progress

- Initial plan recorded before repository inspection.
- First incomplete task identified: `R4 — 审阅 T4`.
- Latest commit `[T4] Export desktop snapshots` is directly relevant, so the review scope is the T4 snapshot export implementation.
- Cross-review found T4 gaps that must be fixed before R4 can be marked done: `AppHost` lacks a headless/no-PTY path, and snapshot export clones all component properties including potentially large values.
- Updated execution plan: add a minimal headless `AppHost` mode for in-memory `snapshot()`/host APIs, constrain snapshot properties to bounded assertion metadata, add regression tests, then run validation.
- Implemented the R4 fixes: `AppHost::new_headless` with in-memory step/snapshot support, bounded component snapshot metadata, and regression tests for headless snapshot plus large collection omission.
- Validation passed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p atto-ui export_snapshot`, `cargo test -p atto-ui headless_apphost_snapshot_uses_in_memory_layout`, and `cargo test --workspace --all-targets`.
- Marked `R4` as `[DONE]` in `TODO.md` with completion notes.
