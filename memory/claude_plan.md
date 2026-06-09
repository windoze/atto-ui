# Execution Plan

## Scope

Complete exactly the first incomplete task listed in `TODO.md`, then stop after updating task records and committing the result.

## Steps

1. Read `TODO.md` first and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check recent repository state only as needed for that task, including the latest commit if it appears directly relevant.
3. Inspect the files and tests tied to the selected task; avoid broad unrelated triage.
4. Implement the task as specified, preserving existing conventions and avoiding workaround behavior.
5. Run formatting, linting, and relevant tests in the required order; fix any unscheduled failures or add the minimum required prerequisite task if a real blocker prevents completion.
6. Update this plan file at key milestones, update `TODO.md` with `[DONE]` and a completion record when the task is actually complete, and update `PLAN.md` only if phase-level planning changes.
7. Commit all task-related changes with a clear message and the required co-author trailer.

## Current Status

Selected first incomplete task: `R29 — 审阅 T29`.

- `TODO.md` index shows all earlier tasks complete and `R29` as the first `TODO` entry.
- `TODO-2.md` review requirements: confirm T29 completion records are concrete and traceable, documentation paths/functions are current, and README content stays user-facing without leaking internal implementation detail.
- Reviewed the T29 docs against startup path handling and command/keymap code.
- Fixed two documentation issues: startup files do not each add parent workspace roots, and README no longer exposes the internal `TabWindow` type for user-facing tab behavior.
- Marked `R29` `[DONE]` in `TODO-2.md` and updated the root `TODO.md` index.
- Final validation passed with `cargo fmt` and `git --no-pager diff --check`.
- This invocation will commit the R29 changes and stop.
