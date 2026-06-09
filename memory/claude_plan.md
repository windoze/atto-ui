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

Completed `T29 — 文档与实施顺序维护`.

- Updated root/editor README content, editor app crate docs, and `PLAN-2.md` status notes.
- Marked `T29` `[DONE]` in `TODO.md` and `TODO-2.md` with a completion record.
- Ran `cargo fmt` and `git --no-pager diff --check`; skipped clippy/test because only Markdown and Rust doc comments changed.
- Next step is to commit the task-related changes and stop.
