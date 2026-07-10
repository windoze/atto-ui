# Execution Plan

I will follow `TODO.md` as the authoritative task list and complete exactly the first task whose heading is not prefixed with `[DONE]`. This file records the working plan and progress updates without exposing private reasoning.

## Steps

1. Read `TODO.md` first and identify the first incomplete task by heading prefix.
2. Check the latest commit message only for an unfinished issue directly relevant to that task.
3. Inspect the task requirements, dependencies, validation requirements, and completion record.
4. Implement the task completely, adding the minimum prerequisite task in `TODO.md` only if a concrete blocker makes the task impossible as written.
5. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and then the relevant/full test suite as required by the task and repository policy.
6. Fix any observed unscheduled test failures, or schedule the minimum prerequisite/follow-up task before marking the current task complete.
7. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record.
8. Update `PLAN.md` only if phase-level sequencing, dependencies, assumptions, or completion criteria change.
9. Commit all task-related changes with a clear message and the required co-author trailer.
10. Stop after this one task.

## Current Status

- First incomplete task identified: `M7.2 流式增量事件`.
- Latest commit is `[M7.1] Add agent provider selection`; it is directly adjacent but does not mention an unfinished issue that changes M7.2 scope.
- Implemented a callback-based DeepSeek streaming API and retained the existing collected `Vec` API as a wrapper.
- Updated the ignored real smoke test to consume incremental events.
- Added a delayed local SSE test proving the first event is emitted before `[DONE]`.
- Targeted DeepSeek client tests pass.
- Workspace `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` pass.
- Marked `M7.2` as `[DONE]` in `TODO.md` with completion and validation records.
- Final diff inspected; next action is to commit the M7.2 task changes.
