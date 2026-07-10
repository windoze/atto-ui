# Execution Plan

## Status

Selected first incomplete task: `M7.3 Async turn 驱动`.

Latest commit `e132540 [M7.2] Add incremental DeepSeek streaming` does not mention an unfinished issue that blocks M7.3.

Implementation boundary: add a live DeepSeek async turn path that streams parsed SSE events through `DeepSeekUiStream` into the existing `AppAction` channel and branch-token checks. Keep full transcript/tool-loop request construction for the already scheduled M7.4/M7.5 tasks.

## Plan

1. Read `TODO.md` and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit only for unfinished work directly relevant to that selected task.
3. Inspect the selected task's requirements, dependencies, validation notes, and relevant code.
4. Implement the selected task fully, adding or updating tests where required.
5. Run formatting, linting, and relevant/full validation in the required order.
6. If validation exposes an unscheduled failure, fix it or add the minimum prerequisite task before marking the current task done.
7. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling in its completion record.
8. Update this progress file at key milestones.
9. Commit all task-related changes with a descriptive message and stop without starting the next task.

## Current Steps

1. Completed: added a DeepSeek turn request/launcher path alongside the existing mock path.
2. Completed: reused `atto-ui-async` to create a Tokio runtime for the live HTTP stream and enabled Tokio I/O in that helper.
3. Completed: routed provider selection so `AgentProvider::DeepSeek` starts the live path and mock/snapshot remains deterministic.
4. Completed: added unit coverage using a local SSE server to verify live events reach the UI through `AppAction`.
5. Completed: ran formatting, clippy, focused live-provider test, full workspace tests, and final format check.
6. Completed: marked `M7.3 Async turn 驱动` as `[DONE]` in `TODO.md` with completion and validation notes.
7. In progress: inspect final diff/status and commit the task changes.

## Validation Notes

- `cargo check -p atto-agent-app --all-targets` passed after implementation.
- `cargo fmt --all` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test -p atto-agent-app deepseek_provider_streams_live_events_through_app_actions` passed.
- `cargo test --workspace --all-targets` passed after updating the PTY help assertion for the provider-neutral `/abort` text.
- `cargo fmt --all -- --check` passed.
