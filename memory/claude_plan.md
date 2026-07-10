## Execution Plan

1. Read `TODO.md` first and identify the first task whose title is not prefixed with `[DONE]`.
2. Check the latest commit only for unfinished work directly relevant to that selected task.
3. Inspect the code and tests needed for that task, avoiding unrelated historical triage.
4. Implement the selected task completely, or add the minimum prerequisite task to `TODO.md` if a concrete blocker makes completion impossible.
5. Run validation in the required order: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant or full test suite as required.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and adding a completion record, or leave it incomplete and document any prerequisite blocker.
7. Update `PLAN.md` only if phase-level sequencing or completion criteria changed.
8. Inspect `git status`, `git diff`, and recent history, then commit all intended changes with a clear task-specific message.
9. Stop after the first incomplete task is completed or after a blocker/prerequisite update is committed.

## Progress

- Initial plan recorded before reading project files or running commands.
- Selected first incomplete task: `M4.7 测试` from `TODO.md`.
- Recent history ends at `[M4.6] Isolate skill tool permissions`; no directly relevant unfinished issue was stated in the latest commit subject.
- Next step is to inspect existing skill unit/PTY coverage and add the minimum missing tests for parsing, matching, size limits, conflict priority, `/skills`, and `/skill`.
- Added a snapshot skill fixture, a unit test for deterministic duplicate-name priority within one skill root, and PTY coverage for listing and activating the fixture skill with `/skills` and `/skill pty-fixture`.
- Next step is targeted validation before running required fmt/clippy/full tests.
- Targeted PTY test initially failed because the assertion looked for text split across a wrapped UI line; adjusted it to wait for a stable same-line fragment.
- Re-run exposed a functional issue: `/skill pty-fixture` opens a no-match slash command popup and does not submit, so the task needs a small input behavior fix rather than a test workaround.
- Fixed parameterized slash input submission by letting no-match slash popup Enter fall through to normal text submit; added a regression test in `atto-ui-chat`.
- Verified targeted input regression, the new conflict-priority unit test, the new `/skill` PTY case, and the full `pty_agent` test file.
- Next step is required full validation: clippy with `-D warnings`, workspace tests, and fmt check.
- Required validation passed: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and `cargo fmt --all -- --check`.
- Updated `TODO.md` with `[DONE] M4.7` and its completion record. Only documentation changed after the green full test run.
- Next step is to inspect git status/diff/log, then commit the intended changes for M4.7 and stop.
- Committed implementation and task record as `56ab547 [M4.7] Add skill registry test coverage`.
