# Execution Plan

I will follow `TODO.md` as the authoritative task list and complete only the first task whose heading is not prefixed with `[DONE]`.

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit message only for directly relevant unfinished work tied to that selected task.
3. Inspect the implementation areas and tests needed for that task.
4. Implement the task without narrowing scope or using workarounds.
5. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and the relevant/full tests required by the task.
6. If unscheduled failures or blockers appear, fix them or add the minimum prerequisite task to `TODO.md`, then stop.
7. Mark the completed task heading with `[DONE]`, update its completion record, and update this file.
8. Commit all task-related changes with a clear message and the required co-author trailer.

Selected task: `M7.4 请求构造接线` — submit/continue turns must build DeepSeek requests from the current transcript via `ContextBuilder`, including skills, file mentions, compact context, tool result feedback, and registered tool schemas, replacing the live path's prompt-only request construction.

Next steps:
1. Check the latest commit message for unfinished work directly tied to M7.4.
2. Inspect the agent app turn-start code, DeepSeek request builders, context builder APIs, and tests around live provider/mock SSE.
3. Wire the DeepSeek provider path to construct requests from the transcript using existing request builder/context APIs and tool registry schema.
4. Add/update focused tests proving live requests include transcript history, skills/file mentions/compact/tool messages, and tool schema.
5. Run formatting, clippy, and tests required by `TODO.md`.
6. Mark M7.4 `[DONE]`, update its completion record and this file, then commit.

Status: M7.4 completed. The live DeepSeek path builds prepared request bodies from the current transcript and active skill state, with direct/execution turns using registered tool schemas and plan turns using the virtual `submit_plan` tool. Focused request-body tests, workspace clippy, full workspace tests, and final rustfmt check passed. `TODO.md` has been updated with the `[DONE]` completion record. Next step is committing the task changes.
