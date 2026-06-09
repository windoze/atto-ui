# Execution Plan

I will follow TODO.md as the authoritative task list and complete only the first task whose heading is not prefixed with `[DONE]`.

## Steps
1. Read TODO.md and identify the first incomplete task without doing broad unrelated triage.
2. Check the latest commit only for unfinished work directly relevant to that task.
3. Inspect the code and tests needed for that task.
4. Implement the task completely, or if a concrete blocker prevents correct implementation, add the minimum prerequisite task to TODO.md and stop.
5. Run formatting, linting, and relevant/full validation according to the task requirements.
6. Update TODO.md by prefixing the completed task heading with `[DONE]` and filling its completion record, or document any blocker/prerequisite while leaving the task incomplete.
7. Update this file at key milestones if the plan changes or a key step completes.
8. Commit all task-related changes with a clear message and stop without starting the next task.

## Current Task

First incomplete task: `#9 状态栏 item 可点击`.

Implementation approach:
1. Preserve `desktop.status` for user custom text/segments.
2. Add a separate internally managed default status bar made from `StatusSegment`s.
3. Wire each default shortcut segment to an internal desktop status command queue.
4. Route status-bar clicks through the custom status bar when present, otherwise through the generated default status bar.
5. Add regression tests for default status rendering and clickable normal-mode shortcuts.

## Progress

- Implemented the default status bar as internal `StatusSegment`s.
- Preserved custom status text/segments by drawing them instead of the generated default status when present.
- Added default status command callbacks for `F10 Menu`, `Ctrl+W Window`, and `F6 Next`.
- Added unit tests and PTY coverage for the new clickable default status behavior.
- Validation completed: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all --all-targets`.
- Updated `TODO.md` to mark `#9` as `[DONE]`.
