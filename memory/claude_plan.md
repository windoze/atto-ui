# Execution Plan

I will use `TODO.md` as the authoritative task list, complete exactly the first task whose heading is not prefixed with `[DONE]`, update its completion record, commit the result, and stop.

## Steps

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit and current worktree only for directly relevant unfinished work or uncommitted changes that must be preserved.
3. Inspect the code, tests, and documentation needed for the selected task.
4. Implement the selected task fully, without workarounds or unrelated changes.
5. Run formatting, linting, and relevant/full tests in the required order; fix or explicitly schedule any unscheduled failures before completion.
6. Mark the completed task heading with `[DONE]` in `TODO.md` and update its completion record. Update `PLAN.md` only if phase-level sequencing changes.
7. Commit all task-related changes with a descriptive message and the required co-author trailer.

## Progress

- Plan refreshed for this invocation.
- First incomplete task selected from `TODO.md`: `R23` / `审阅 T23` in `TODO-2.md`.
- Latest commit is `[T23] Add explorer context menu and inline file edits`, directly relevant to `R23`.
- Worktree already had unrelated untracked files `notification.sh` and `run_agent.sh`; I will avoid touching them unless they become directly relevant.
- Detailed `R23` requirements inspected: validate unsafe names/existing targets, map refresh after rename/new, and right-click isolation from left-click selection/open behavior.
- Review found a directly relevant existing-target gap: `Path::exists()` misses dangling symlinks, allowing rename to overwrite a dangling symlink entry on Unix.
- Implemented shared target availability checks with `symlink_metadata` and added integration coverage for dangling symlink rejection, post-commit open path refresh, and right-click selection isolation.
- Validation completed successfully: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- No `tools/run_fixtures.py` fixture runner exists in this repository.
- Marked `R23` as `[DONE]` in `TODO.md` and `TODO-2.md` with a completion record.
- Next step: review the final diff and commit this task.
