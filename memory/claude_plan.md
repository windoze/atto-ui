# Execution Plan

I will avoid recording private chain-of-thought, but will keep this file updated with a concise plan, decisions, and progress.

1. Read TODO.md first and identify the first task whose title is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the selected task's requirements, dependencies, validation instructions, and relevant code.
4. Implement the task completely without changing unrelated behavior or working around spec gaps.
5. Run formatting, clippy with warnings denied, and relevant/full tests as required by the task.
6. If validation reveals unscheduled failures, fix them or add the minimum prerequisite task in TODO.md before the current task.
7. Mark the completed task title with `[DONE]`, update its completion record, and update PLAN.md only if phase-level sequencing changed.
8. Commit all changes for this task with a clear message and stop without starting the next task.

Progress:
- Plan initialized.
- Selected first incomplete task from TODO.md: "用 `snapshot_app` 抓屏与参考截图人工比对。"
- Next: inspect the task context, available reference assets, and latest commit for any directly relevant unfinished work.
- Confirmed no reference image assets are checked into the repository; used `UI_GAPS.md` as the written comparison criteria.
- Captured the default `snapshot_app` 80x24 screen and File-menu-open screen with a temporary PTY test harness, then removed the temporary test file.
- Updated `TODO.md` to mark the task `[DONE]` with the comparison findings. Final repository changes are documentation/progress only, so the previous full green validation is reused.
- Confirmed `TODO.md` has no remaining `- [ ]` tasks. Preparing the final project completion commit and `v0.1.0` tag.
