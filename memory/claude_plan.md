# Current Invocation Plan

## Reasoning Summary

- `TODO.md` is the authoritative source for the next executable task and completion state.
- A task is complete only when its heading/title is explicitly prefixed with `[DONE]`.
- This invocation must complete exactly the first incomplete task, or record and commit the minimum prerequisite/blocker update if completion is impossible.
- I will avoid broad historical triage before selecting the current task.
- I will not use workarounds or weaken task requirements to make progress.

## Step-By-Step Execution Plan

1. Read `TODO.md` to identify the first task whose title is not prefixed with `[DONE]`.
2. Review the selected task body, dependencies, validation requirements, and completion-record expectations.
3. Inspect only the code, tests, and documentation relevant to that selected task.
4. Implement the selected task completely, using small targeted patches.
5. Run `cargo fmt`.
6. Run `cargo clippy --all-targets -- -D warnings`.
7. Run the task-required tests, and run the full test suite if code changed and no narrower validation is sufficient.
8. If any unscheduled failing test or fixture is observed, fix it or add the minimum required prerequisite task to `TODO.md` before marking the current task complete.
9. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling in its completion record.
10. Update `PLAN.md` only if phase-level sequencing, dependencies, assumptions, or completion criteria changed.
11. Commit all relevant changes with a clear task-specific message.
12. Stop without starting the next task.

## Progress Log

- Initial plan written before reading project task files.
- Identified first incomplete task: `R15 — 审阅 T15`.
- Latest commit is `[T15] Add id indexes for window and tree ops`; no unfinished issue is mentioned in the latest commit title.
- Working tree has this plan file modified plus unrelated untracked scripts (`notification.sh`, `run_agent.sh`) that will not be touched unless required by the task.
- R15 review found blocking issues to fix before marking complete:
  - `WindowManager::windows_mut()` and public `Window::id` can let callers stale or dangle `window_index`.
  - Runtime `ViewPathIndex` paths are not always spec paths; transparent wrappers such as `Visibility` can make incremental replacement mutate the wrong live node.
  - `ComponentTree::apply_ops_incremental` assigns the new root before view-side validation/build, so errors can leave root spec and live view inconsistent.

## Revised Execution Plan

1. Make window ids read-only outside the crate and prevent public unsynchronized mutable access to the whole window slice.
2. Add targeted regression tests for window index immutability/synchronization boundaries.
3. Change runtime incremental updates so replacement of nodes after unsupported property/event changes uses the updated spec subtree by id/tag instead of assuming view paths equal spec paths.
4. Add rollback behavior for incremental view-side failures, keeping both root spec and live view at their original state on error.
5. Add regression tests covering transparent `Visibility` paths and unknown-component failure rollback.
6. Run formatting, clippy, and tests before marking R15 complete.

## Progress Log Update

- Implemented window-index hardening: `Window::id()` is now read-only outside the crate, `windows_mut()` is no longer public API, and `window_index_of` falls back to a linear scan if a cached index is stale.
- Implemented runtime incremental hardening: incremental path-index updates now require live view shape to match the spec shape; wrapper mismatches fall back to rebuilding the final root, and view-side errors roll back to the original root/view.
- Added regression tests for stale internal window slice reorder recovery, transparent `Visibility` incremental event binding, unknown component rollback, batch partial rollback, and invalid property rollback.
- Targeted verification passed: `cargo test window_index --lib`; `cargo test component_tree_incremental --lib`.
- Extended the fix to spec-level/apply-and-rebuild atomicity: `apply_tree_ops` now preserves the input tree on batch failure, and `ComponentTree::apply_ops_and_rebuild` swaps root/view only after build succeeds.
- Final verification passed: `cargo fmt`; targeted rollback tests; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --all --all-targets`.
- Updated `TODO.md` to mark `R15` as `[DONE]` with a completion record.
