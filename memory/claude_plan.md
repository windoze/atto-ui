# Execution Plan

I will follow `TODO.md` as the authoritative task list and complete exactly the first task whose title is not prefixed with `[DONE]`.

1. Read `TODO.md` to identify the first incomplete task and its stated validation requirements.
2. Check the latest commit only for unfinished work directly relevant to that selected task.
3. Inspect the code paths and tests needed for that task.
4. Implement the task without changing unrelated behavior or working around specification gaps.
5. Run formatting, linting, and the relevant test suite in the required order.
6. If validation exposes an unscheduled failure, fix it or add the minimum prerequisite task in `TODO.md`.
7. Mark the completed task with `[DONE]` in `TODO.md` and update its completion record.
8. Commit all changes for this invocation, then stop without starting the next task.

## Progress

- Plan refreshed.
- Read `TODO.md`; selected first incomplete task `M7.2 keypad 模式`.
- Checked latest commit `9d69d7b [M7.1] Render terminal cursor shapes`; no directly relevant unfinished blocker found.
- Inspected terminal key encoding, vt100 `application_keypad()`, crossterm keypad-origin state, and existing application cursor tests.
- Implemented application keypad encoding for crossterm keypad-origin events and added regression tests.
- Ran `cargo fmt --all`.
- Ran `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`; both passed.
- Ran focused keypad regression tests; they passed.
- Ran `cargo test --workspace --all-targets`; it passed.
- Marked `M7.2 keypad 模式` as `[DONE]` in `TODO.md` with the completion and validation record.
- Tightened `KeypadBegin` handling so the dedicated keypad key works even when crossterm does not attach `KEYPAD` state; rerunning validation.
- Final validation after the adjustment passed: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, focused keypad tests, and `cargo test --workspace --all-targets`.
