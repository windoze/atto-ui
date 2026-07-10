## Execution plan

I will follow `TODO.md` as the authoritative task list and complete only the first task whose heading is not prefixed with `[DONE]`.

1. Read `TODO.md` first to identify the first incomplete task and its validation requirements.
2. Inspect only the files needed for that task, plus the latest commit if it is directly relevant to the selected task.
3. Implement the task without narrowing scope or using workarounds.
4. Run formatting, linting, and relevant tests in the required order, fixing any unscheduled failures that appear.
5. Mark the completed task title with `[DONE]` in `TODO.md` and update its completion record.
6. Update this file at key milestones.
7. Commit all changes for the completed task with a clear message and stop.

## Current task

Selected first incomplete task: `M7.8 测试与冒烟`.

Task scope:
1. Keep default tests on mock provider and local mock SSE server only.
2. Extend the ignored `deepseek_real_smoke` to cover one real end-to-end turn with text streaming and at least one tool round trip.
3. Confirm default CI/test runs do not require external network access.
4. Mark `M7.8` done in `TODO.md` after validation and commit the changes.

## Progress

- Implemented the ignored real DeepSeek smoke as a two-request round trip: force `atto_smoke_echo`, append the local tool result, then verify the final streamed response.
- Validation completed: formatting, clippy, default ignored-smoke check, full workspace tests, final format check, and the manual ignored real DeepSeek smoke all passed.
- Next: commit the M7.8 changes and stop.
