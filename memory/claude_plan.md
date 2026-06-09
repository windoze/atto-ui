# Execution Plan

I will follow `TODO.md` as the source of truth and complete exactly the first task whose heading is not prefixed with `[DONE]`.

## Steps

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit message only for directly relevant unfinished work tied to that task.
3. Inspect the files and tests related to that task.
4. Implement the task without changing unrelated behavior or using workarounds.
5. Run formatting, linting, and the relevant/full validation required by the task.
6. If any unscheduled test or fixture failure appears, fix it or add the minimum prerequisite task in `TODO.md` before stopping.
7. Mark the completed task heading in `TODO.md` with `[DONE]` and update its completion record.
8. Commit all changes for this task with a clear message and the required co-author trailer.
9. Stop after the commit without starting the next task.

## Progress

- Created initial execution plan.
- Identified first incomplete task: `R22` / `审阅 T22` in `TODO-2.md`; the latest commit is `[T22] Add file tree git status and multi-select`, which is directly relevant to the review.
- Reviewed the T22 implementation against the R22 checks and found/fixed a runtime compatibility regression where setting the legacy `selection` property could leave stale multi-selected ids.
- Added regression coverage for the legacy FileTree runtime schema property list and for `selection` resetting the multi-select set.
- Validation completed successfully with `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Marked `R22` as `[DONE]` in `TODO.md` and `TODO-2.md` with the review completion record.
