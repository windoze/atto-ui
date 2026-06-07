# Claude Execution Plan

## Scope
- Follow `TODO.md` as the authoritative task list.
- Complete exactly the first task whose heading is not prefixed with `[DONE]`, then stop.
- Do not perform broad historical triage before selecting that task.

## Execution Plan
1. Read `TODO.md` and identify the first incomplete task by heading prefix.
2. Review the selected task's requirements, dependencies, validation requirements, and completion-record format.
3. Inspect only the code and tests needed for that task, plus recent Git context if it directly affects the selected task.
4. Implement the smallest spec-correct change required by the task.
5. Run formatting first, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full tests required by the task.
6. If an unscheduled failing test or fixture appears, fix it if in scope or add the minimum prerequisite task to `TODO.md` before the blocked task and stop.
7. Update this file after key milestones or any plan change.
8. Mark the task title in `TODO.md` with `[DONE]` only after implementation and required validation succeed, and update its completion record.
9. Commit all changes for this invocation with a clear task-specific commit message.
10. Stop without starting the next task.

## Current Status
- `TODO.md` read. First incomplete task is `R5 审阅 T5` in `TODO-2.md`.
- Detailed T5/R5 requirements read. Review checklist: no residual app-level Explorer work-area calculation, Explorer close/reopen preserves dock side/size, editor commands fallback to last focused editor when Explorer has focus, and `atto-editor-app` plus related PTY tests pass.
- Implementation inspection found no residual `work_without_explorer` / manual Explorer reserve helpers. Added focused regression tests for Explorer dock side/size preservation across close/reopen and `active_editor_commands` fallback while Explorer is focused.
- Validation passed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p atto-editor-app`, and `cargo test --workspace --all-targets`.
- `TODO.md` and `TODO-2.md` updated to mark `R5 审阅 T5` complete with the completion record.
- Git status/diff/log inspected. Intended commit contents are `TODO.md`, `TODO-2.md`, `crates/atto-editor-app/src/app.rs`, and `memory/claude_plan.md`; unrelated untracked `notification.sh` and `run_agent.sh` remain unstaged.
- Final step for this invocation: commit the R5 review changes, then stop.
