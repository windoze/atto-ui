# Current Invocation Plan

## Scope

- Follow `TODO.md` as the authoritative task list.
- Complete exactly the first incomplete task, then stop after committing.
- Do not perform broad historical triage before identifying that task.

## Execution Steps

1. Read `TODO.md` and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the task requirements, dependencies, and validation instructions in `TODO.md`.
4. Examine only the code and tests needed to implement the selected task correctly.
5. Implement the task with small targeted patches, avoiding workarounds or scope weakening.
6. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite required by the task.
7. If a real blocker or unscheduled failing test is found, update `TODO.md` with the minimum prerequisite task and stop after committing that bookkeeping.
8. If the task is completed, mark its heading in `TODO.md` with `[DONE]` and update its completion record with changed files and validation results.
9. Commit all intended changes with a clear task-specific commit message.
10. Stop without starting the next task.

## Progress Log

- Plan initialized before reading project task files.
- Identified first incomplete task: `T1 — test-host 输入与断言能力补全（A.1）`.
- Latest commit `e324624 [R15] Review id indexes` is not directly relevant to T1, so no prerequisite was added from commit history.
- Next focus: inspect `crates/atto-ui-test-host` and existing PTY tests, implement the missing input/assertion APIs, add coverage, run required validation, update `TODO.md`, and commit.
- Inspection found `PtyTestHost` currently lacks access to the PTY master after spawn, so `resize(cols, rows)` will store the master handle and update both the kernel PTY size and vt100 parser size.
- Test strategy: add a deterministic `snapshot_app --input-api` fixture that echoes crossterm key/mouse/resize events, then add a PTY integration test that calls the new host APIs directly.
- Implemented the host API expansion and the `pty_test_host_input_resize_and_snapshot_apis` integration test. Validation starts with formatting, then clippy, then tests per repository policy.
- Validation completed successfully: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted `cargo test --test pty_test_host_api`, and full `cargo test` all passed.
- `TODO.md` now marks T1 as `[DONE]` and records implemented APIs plus validation results. Intended T1 files are staged for commit; unrelated pre-existing worktree changes remain unstaged.
