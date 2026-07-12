# Autonomous Task Execution Plan

## Current objective
Complete exactly the first incomplete task in `TODO.md`, update task bookkeeping, validate the work, commit the resulting changes, and stop.

## Execution steps
1. Read `TODO.md` to identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished issues directly relevant to that task.
3. Inspect the files and tests relevant to the selected task.
4. Implement the task as written, without narrowing scope or using workarounds.
5. Run formatting, linting, and the relevant tests, escalating to the full required suite when appropriate.
6. If an unscheduled test failure or blocking prerequisite is found, update `TODO.md` with the minimum prerequisite task and stop after committing that bookkeeping.
7. If implementation succeeds, prefix the task title in `TODO.md` with `[DONE]`, update its completion record, and update `PLAN.md` only if the phase-level plan changed.
8. Commit all changes for this task with a clear task-scoped message and the required co-author trailer.

## Progress log
- Initialized plan before inspecting project tasks.
- Identified first incomplete task: M7.5 配置生效接线.
- Next: inspect terminal config, terminal emulator hard-coded settings, and settings apply/save wiring.
- Implemented component-level TerminalConfig application for scrollback, palette, release/prefix shortcuts, alternate-screen scroll keys/step/enabled state, shell integration, and default cursor shape.
- Wired TerminalPaneGroup and both terminal app shells to apply changed settings bindings to live terminal panes and to build new/restarted panes from the active config.
- Added targeted tests for live config application, palette rendering, and pane-prefix config propagation.
- Validation completed: formatting, workspace clippy, targeted terminal config tests, and full workspace tests passed.
- Marked M7.5 as [DONE] in TODO.md with completion and validation notes.
- Next: review diff, commit all task changes, then stop.
- Spot-check found one constructor field still using an enum default; corrected it to use the computed runtime config.
- Re-ran formatting, workspace clippy, targeted config tests, and the full workspace test suite successfully after the correction.
- Added a validation guard before pane-group live config mutation and reran formatting, clippy, targeted tests, and the full workspace test suite successfully.
- Ran `cargo fmt --all -- --check` successfully and updated the TODO validation record; only documentation/bookkeeping changed after the last full test run.
