# Execution Plan

I will use TODO.md as the authoritative task list, identify the first task whose heading is not prefixed with [DONE], complete exactly that task, validate it according to its requirements, update TODO.md with a completion record and [DONE] prefix, commit the resulting changes, and stop.

Steps:
1. Read TODO.md first to find the first incomplete task and its validation requirements.
2. Check the latest commit only for unfinished work directly relevant to that selected task.
3. Inspect the minimal code and documentation needed for the selected task.
4. Implement the task without narrowing scope or using workarounds.
5. Run formatting, clippy with warnings denied, and the required tests in the requested order.
6. If an unscheduled blocking failure appears, fix it or add the minimum prerequisite task in TODO.md and stop after committing.
7. Mark the completed task title with [DONE], update its completion record, commit all task-related changes, and stop before starting any next task.

Progress:
- Selected first incomplete task: R19 — review T19 Signature Help.
- Review found a task-relevant race: stale completion responses can clear a newer signature help popup because completion_requested_position is cleared before the response handler's stale-position guard runs.
- Implemented fix: completion responses now take and compare their requested cursor position before mutating completion/signature popup state; stale responses return without clearing an active signature help popup.
- Added regression test `stale_completion_response_does_not_clear_signature_help_popup`.
- Validation passed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted stale completion/signature help tests, and `cargo test --workspace --all-targets`.
- R19 marked done in TODO.md and TODO-2.md. Next step: commit the task changes and stop.
