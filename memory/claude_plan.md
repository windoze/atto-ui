# Execution Plan

I will follow TODO.md as the authoritative task list and complete exactly the first task whose heading is not prefixed with `[DONE]`. I will not perform broad triage before selecting that task.

## Steps
1. Read TODO.md to identify the first incomplete task and its requirements, dependencies, validation instructions, and completion record expectations.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the minimum relevant project files needed to implement the selected task correctly.
4. Implement the task as written, avoiding workarounds or scope narrowing.
5. Run formatting, clippy with warnings denied, and the relevant/full tests required by the task and repository policy.
6. If an unscheduled blocking issue or failing test is found, either fix it immediately or add the minimum prerequisite task to TODO.md before the blocked task, commit that bookkeeping, and stop.
7. When the task is complete, update TODO.md by prefixing the task heading with `[DONE]` and filling the completion record with implementation and validation notes.
8. Commit all task-related changes with a clear task-specific message and the required co-author trailer.
9. Stop after this single task.

## Progress Log
- Plan created before selecting or executing the current TODO task.
- Selected first incomplete task: `#8 滚动条箭头/轨道` in TODO.md. Scope is composable scroll rendering so vertical scrollbar arrow end caps and `░` track render even when the content area is short.
- Implemented the shared scrollbar layout change so a one-cell track remains visible instead of being fully covered by a thumb, and short ListBox/TableView border scrollbars expand to enough edge cells for arrow caps plus track. Added focused unit regressions for shared layout, short list rendering, and short table rendering.
- Validation completed with formatting, focused scrollbar unit tests, clippy with warnings denied, and the full Rust test suite. No separate fixture runner was present. Updated TODO.md to mark `#8` as `[DONE]`.
