# Execution Plan

I will follow `TODO.md` as the authoritative task list and complete only the first task whose heading is not prefixed with `[DONE]`.

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit only for directly relevant unfinished work tied to that task.
3. Inspect the affected code and tests for the selected task.
4. Implement the task completely, avoiding unrelated changes and avoiding workarounds.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite required by the task.
6. If tests expose unscheduled failures, fix them or add the minimum prerequisite task before marking the current task complete.
7. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling in its completion record.
8. Update this progress file at key milestones.
9. Commit all task-related changes with a descriptive message and the required co-author trailer.
10. Stop after the commit without starting the next task.

## Progress

- Plan file refreshed.
- Selected first incomplete task: `M3.5 测试` in `TODO.md`.
- Inspection found existing PTY coverage for `prefix+F10`, `prefix+w`, and `prefix+z`; remaining coverage is `prefix+prefix` to a child process, non-terminal global shortcut routing, and capture-release global F10 routing.
- Added PTY tests for literal prefix forwarding and direct global shortcut routing from both non-terminal focus and released terminal capture.
- Validation completed: targeted terminal PTY tests, workspace formatting check, clippy with warnings denied, and full workspace tests all passed.
- `TODO.md` updated to mark `M3.5` as `[DONE]`.
