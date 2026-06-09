# Execution Plan

I will complete exactly the first incomplete task from `TODO.md` and stop after committing it. I will not reveal private chain-of-thought; this file records the actionable plan and progress.

## Steps

1. Read `TODO.md` first and identify the first task whose title is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the files needed for that task and determine whether it can be completed as written.
4. If a concrete prerequisite blocks the task, update `TODO.md` with the minimum prerequisite task, commit that bookkeeping, and stop.
5. Otherwise, implement the selected task fully using repository conventions.
6. Run formatting, clippy with warnings denied, and the relevant/full test suite required by the task.
7. If any unscheduled test or fixture failure appears, fix it or schedule it explicitly before marking the task complete.
8. Mark the task `[DONE]` in `TODO.md` and update its completion record.
9. Commit all changes for this task with a clear message and stop without starting the next task.

## Progress

- Plan file created.

- Selected first incomplete task: `#7 热键字母配色`.
- Latest commit has no unfinished note directly relevant to this task.
- Inspecting menu rendering/theme token wiring before editing.

- Spec confirmed from `UI_GAPS.md`/`PLAN.md`: mnemonic glyphs should use the `menu-mnemonic` accent token, defaulting to classic red.
- Implementation plan: set the default token to red accent while preserving theme overlays; add renderer/theme tests for top-level and dropdown mnemonic cells.

- Code changes applied: `menu-mnemonic` now defaults to classic red, with renderer and theme-token regression tests added.
- Running validation: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then `cargo test --all --all-targets`.

- Validation completed successfully.
- `TODO.md` updated with `[DONE] #7` and a completion record.
- Preparing final diff review and commit.
