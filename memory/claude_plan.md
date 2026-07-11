## Execution plan

I will use `TODO.md` as the authoritative source and complete exactly the first task whose heading is not prefixed with `[DONE]`. I will not do broad issue triage before selecting that task.

1. Read `TODO.md` to identify the first incomplete task, including its requirements, dependencies, validation steps, and completion record expectations.
2. Check the latest commit message only for directly relevant unfinished work tied to that selected task.
3. Inspect the minimum relevant project files needed to understand and implement the selected task.
4. Implement the task completely without narrowing scope or introducing workarounds.
5. Update tests or documentation that are directly required by the task.
6. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test command required by `TODO.md`.
7. If any observed test failure is not already scheduled, fix it or add the minimum prerequisite/follow-up task in `TODO.md` before marking the current task done.
8. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record with the implementation and validation outcome.
9. Update this plan file at key milestones or if the plan changes.
10. Commit all task-related changes with a clear task-specific commit message and the required co-author trailer, then stop.

## Current task

Selected first incomplete task: `M4.R Review`.

Relevant latest commit: `[M4.7] Add terminal PTY coverage`; it is directly related to M4 test coverage and does not mention unfinished work that changes the selected task ordering.

Review focus:

1. Inspect terminal selection hit testing and text extraction, especially wide-character handling.
2. Inspect wheel routing for the required three-way decision tree: mouse reporting, alternate screen, then local scrollback.
3. Inspect clipboard behavior and dependency configuration to ensure the default/first-version behavior remains safe and portable.
4. Run formatting, clippy, and workspace tests after any needed fixes.
5. Mark `M4.R Review` as `[DONE]`, update its completion record, commit, and stop.

## Progress

- Identified a review coverage gap: wide-character highlighting is tested, but selected-text extraction needs direct coverage when the selection intersects only the leading or continuation half of a wide cell.
- The new regression failed, confirming a bug in selected-text extraction for partial wide-cell ranges. I will normalize extraction columns to full wide-cell boundaries before calling `vt100::Screen::contents_between`.
- Fixed wide-cell text extraction, added unit/component regressions, completed `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` successfully. `TODO.md` now marks `M4.R Review` as done.
