# Claude Execution Plan

## Scope

- Follow `TODO.md` as the authoritative task list.
- Identify the first task whose heading is not prefixed with `[DONE]`.
- Complete exactly that one task, then stop after committing the result.

## Initial Steps

1. Read `TODO.md` to find the first incomplete task and its validation requirements.
2. Check the latest commit only for directly relevant unfinished work after the task is identified.
3. Inspect the code paths and tests needed for that task.
4. Implement the smallest spec-correct change that fully satisfies the task.
5. Run formatting first, then clippy with warnings denied, then the relevant/full tests required by the task.
6. If an unscheduled blocking failure appears, either fix it or add the minimum prerequisite task to `TODO.md` and stop.
7. Mark the task heading `[DONE]`, update its completion record, and update this plan with progress.
8. Inspect git status/diff/log, stage only intended files, commit with a task-specific message, and stop.

## Progress Log

- Plan initialized before task discovery.
- First incomplete task identified: `NR3 — 审阅 NT3` in `TODO-1.md`.
- Latest commit is `[NT3] Add Node id handles and error mapping`; no explicit unfinished issue was present in the commit summary.
- Review found that `anyhow::Error` conversion preserved only the outer display message, dropping source-chain context. This is in scope for NR3 and will be fixed with tests.
- Review will also add handle lifecycle tests for stale handles after release/reallocation and cross-namespace rejection.
- `cargo clippy --workspace --all-targets -- -D warnings` first run failed on an unused test import introduced by the review fix; remove the import and rerun validation.
- Implemented the NR3 review fixes: `anyhow::Error` conversion now preserves the display source chain, and id handle tests cover stale handles after release/reallocation plus namespace rejection.
- Validation passed: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --all --all-targets`.
- Marked `NR3` done in `TODO.md` and `TODO-1.md` with the completion record. No `PLAN.md` update is needed because phase ordering did not change.

## NR3 Review Plan

1. Inspect `crates/atto-ui-node/src/ids.rs`, `src/error.rs`, and the conversion call sites changed by NT3.
2. Verify string handle semantics for `CallbackId` and `WindowId`, including reuse, release, invalid-handle behavior, and namespace separation.
3. Verify error mapping preserves useful messages while not exposing unnecessary internals.
4. Check Rust tests for handle lifecycle and error conversion coverage; add or fix tests/code if the review finds gaps.
5. Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --all --all-targets` unless a blocker requires updating `TODO.md` first.
6. Mark `NR3` as `[DONE]` with a completion record, update the index in `TODO.md`, commit all intended changes, and stop.
