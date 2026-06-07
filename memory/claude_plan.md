# Current Invocation Plan

I will not record hidden chain-of-thought, but this file captures the complete actionable execution plan, decisions, progress, and validation status for this invocation.

## Goal

- Complete exactly the first incomplete task in `TODO.md`, then stop.
- Treat `TODO.md` as the authoritative task list and only update `PLAN.md` if phase-level sequencing or completion criteria change.

## Execution Plan

1. Read `TODO.md` and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the code and tests relevant to the selected task without doing broad unrelated triage.
4. Implement the selected task completely, using small targeted patches.
5. Run required validation in the requested order: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task and by observed failures.
6. If validation reveals an unscheduled failing test/fixture, fix it if in scope or add the minimum prerequisite/follow-up task in `TODO.md` before marking the current task complete.
7. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling in its completion record.
8. Update this progress file whenever a key step completes or the plan changes.
9. Inspect git status/diff/log, then commit all intended changes with a descriptive task-specific message.
10. Stop after the commit without starting the next task.

## Progress

- Plan initialized before reading task files or running commands.
- Read `TODO.md` and `TODO-2.md`; selected first incomplete task: `T5 — C2 atto-editor-app Explorer 改用 WM Docking`.
- Checked latest commit summary: `[R4] Review dock resize and auto-hide hit-test`; no explicit unfinished issue directly relevant to T5.
- Inspected `crates/atto-editor-app/src/app.rs`, `actions.rs`, existing app tests, and WM docking API. Current app still has local `ExplorerDock`, `default_explorer_rect`, `docked_explorer_rect`, and `work_without_explorer`; implementation will replace those with `DockSide`/`WindowDock` and store only last Explorer dock size.
- Implemented Explorer docking migration: app creation/toggle/left-right actions now use `WindowDock`, app state tracks `DockSide` plus last dock size, and the old Explorer rect/work-area helpers were removed. Added unit tests and a new integration test file for Explorer dock reserve/side/resize behavior.
- Full test run exposed `explorer_enter_open_smoke` coordinate drift from the new edge-aligned dock layout. Updated existing Explorer PTY click coordinates from row 5 to row 4 so they target the same file-tree row under WM docking.
- Validation passed after fixes: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace --all-targets`.
- Marked T5 as `[DONE]` in `TODO-2.md`, updated the `TODO.md` index row to `DONE`, and recorded implementation plus validation details.
- Reviewed and staged intended T5 files only; unrelated untracked `notification.sh` and `run_agent.sh` remain unstaged.
