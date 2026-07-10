## Execution plan

1. Read `TODO.md` first to identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit only for directly relevant unfinished work tied to that task.
3. Inspect the task's referenced files and nearby implementation/tests.
4. Implement the task exactly as specified, adding or updating tests where required.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task and repository policy.
6. Update `TODO.md` with a `[DONE]` prefix and a completion record if the task is complete; otherwise add the minimum prerequisite task and stop.
7. Commit all task-related changes with a descriptive message and the required co-author trailer.
8. Stop after completing exactly this one task.

## Current task

First incomplete task: `M7.5 真实 tool loop`.

Plan for this task:

1. Inspect the existing DeepSeek provider turn driver, stream mapper, tool execution/approval gate, plan gate, and budget code.
2. Reuse the existing tool-call handling path after `finish_reason = tool_calls` so completed tool calls are executed or queued for approval exactly like mock/UI tool calls.
3. After tool results are available, automatically build the next live DeepSeek request from the updated transcript and continue the loop until the assistant finishes without tool calls or the existing request/tool budgets stop the turn.
4. Add focused tests using the local mock SSE server/client path to cover live tool-call continuation, tool result backfill, and budget/termination behavior.
5. Run formatting, clippy with warnings denied, and the required test suite.
6. Mark `M7.5` done in `TODO.md`, update its completion record, commit the changes, and stop.

## Progress

- Identified `M7.5 真实 tool loop` as the active task.
- Implemented live DeepSeek continuation after completed tool results while keeping mock and plan-mode tool-call behavior unchanged.
- Added focused tests for an allowed read-only tool loop, denied approval feedback loop, and model-request budget termination.
- Ran required validation and marked `M7.5` complete in `TODO.md`; next step is committing these changes.
