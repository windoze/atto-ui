# Execution plan

I will follow `TODO.md` as the source of truth, complete exactly the first task whose heading is not prefixed with `[DONE]`, update the task record, commit the resulting changes, and stop.

## Step-by-step plan

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the files and code paths needed for that task, avoiding unrelated historical triage.
4. Implement the requested change completely, preserving existing conventions and avoiding workarounds.
5. Run formatting, linting, and the smallest relevant tests first; escalate to the full suite if required by the task or by observed failures.
6. If a blocking prerequisite is discovered, update `TODO.md` with the minimum new prerequisite task, commit that bookkeeping, and stop.
7. If implementation succeeds, mark the selected task title with `[DONE]`, update its completion record with the meaningful changes and validation result, and update `PLAN.md` only if phase-level planning changed.
8. Commit all task-related changes with a clear task-specific message, then stop without starting the next task.

## Progress log

- Created this plan before inspecting task details.
- Identified the first incomplete task as `M4.7 测试`: add PTY coverage for mouse selection/copy, copy-mode selection copy, terminal app scroll-wheel routing for mouse-reporting apps, alternate-screen apps, popup-style apps, and main-screen scrollback.
- Added PTY coverage in `crates/atto-ui-terminal/tests/pty_terminal_window_interactions.rs` for mouse drag copy, Shift-drag copy with mouse reporting, app-like wheel routing branches, and main-screen local scrollback; the terminal window PTY suite passed.
- Completed validation with formatting, clippy, targeted PTY tests, and the full workspace test suite; marked `M4.7` as `[DONE]` in `TODO.md`.
