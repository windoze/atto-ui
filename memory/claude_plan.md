# Execution Plan

## Current objective

Complete exactly the first incomplete task listed in `TODO.md`, mark it `[DONE]`, update its completion record, validate the change according to the task requirements, commit the result, and stop.

## Constraints

- `TODO.md` is authoritative for task order and completion state.
- A task is complete only when its heading/title is prefixed with `[DONE]`.
- Do not proceed to the next task after completing the first incomplete one.
- Do not split or reorder tasks unless a concrete prerequisite blocker makes the current task impossible.
- Fix or explicitly schedule any observed unscheduled test failure before marking the current task done.
- Use `PLAN.md` only for phase-level plan changes, not routine bookkeeping.
- Include all relevant uncommitted files in the final commit for this invocation.

## Step-by-step plan

1. Read `TODO.md` to find the first task whose title is not prefixed with `[DONE]`.
2. Inspect the latest commit message for any explicitly unfinished issue directly relevant to that task.
3. Read the task details and the relevant source/test files needed for that task.
4. Implement the task as written, without narrowing the scope or introducing workarounds.
5. Add or update targeted tests for the implemented behavior.
6. Run formatting and validation in the required order: `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task.
7. If validation reveals an unscheduled failure, fix it if in scope or add the minimum prerequisite/follow-up task in `TODO.md` before completing.
8. Update `TODO.md`: prefix the completed task title with `[DONE]` and update its completion record with implementation and validation notes.
9. Update this progress file at key milestones.
10. Review the working tree and commit all changes for this completed task with a descriptive message and required co-author trailer.

## Progress log

- Created initial execution plan.
- Identified first incomplete task: `M1.R Review` in `TODO.md`.
- Reviewed the terminal lifecycle/callback implementation and preserved capture, paste, mouse forwarding, and scrollback behaviors.
- Validation passed: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Updated `TODO.md` to mark `M1.R Review` as `[DONE]` with completion notes.
