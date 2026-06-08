# Execution Plan

I will follow `TODO.md` as the source of truth and complete exactly the first task whose heading is not prefixed with `[DONE]`.

## Steps
1. Read `TODO.md` and identify the first incomplete task, treating only `[DONE]`-prefixed task headings as complete.
2. Check the latest commit message for any explicitly unfinished issue that is directly relevant to that task.
3. Inspect only the files and task context needed to implement that task.
4. Implement the task without changing unrelated behavior or using workarounds.
5. Run formatting, linting, and the relevant tests required by the task; address any unscheduled failures as required by the task policy.
6. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling in its completion record. Update `PLAN.md` only if the phase-level plan changes.
7. Update this plan file at key milestones.
8. Commit all task-related changes with a descriptive message and the required co-author trailer.
9. Stop after this one task.

## Milestone: Task Identified

The first incomplete task is `R17` in `TODO-2.md`: review of `T17` (`Workspace / LSP Bridge 状态层`). I will review the completed T17 implementation against its TODO/PLAN acceptance criteria, fix any review-blocking defects found, update the task record, validate, commit, and stop.

## Milestone: Review Finding

Review found a T17 bridge bug beyond the checklist: `LspWorkspaceBridge::poll` iterates every `(workspace_root, language_id)` sync but each `LspWorkspaceSync::poll_workspace` uses the single global active workspace buffer. When multiple LSP syncs exist, inactive-language/root sessions can be polled against a buffer they do not track. I will patch polling so each sync is only polled when the active workspace buffer belongs to that sync, and add a focused test.

## Milestone: Validation Complete

R17 review fixes are implemented. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` passed. `TODO.md` and `TODO-2.md` now mark R17 as `[DONE]`.
