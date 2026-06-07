# Claude Execution Plan

## Scope

- Follow `TODO.md` as the authoritative task list.
- Complete exactly the first task whose heading is not prefixed with `[DONE]`, then stop.
- Do not perform broad triage before identifying that task.
- Keep this file updated when the plan changes or a key step is completed.

## Execution Plan

1. Read `TODO.md` and identify the first incomplete task by heading prefix.
2. Read only the immediately relevant project files needed to understand that task.
3. Check the latest commit message only for unfinished work directly relevant to the selected task.
4. Implement the selected task as written, without narrowing scope or introducing workarounds.
5. If a concrete blocker prevents correct implementation, update `TODO.md` with the minimum prerequisite task, keep the current task incomplete, commit that bookkeeping, and stop.
6. Run validation in the requested order when code changes are made: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task.
7. If any unscheduled test or fixture failure is observed, either fix it or add the minimum scheduled task before marking the current task complete.
8. Mark the task complete by prefixing its `TODO.md` title with `[DONE]` and updating its completion record.
9. Commit all changes related to this invocation with a clear task-specific message.
10. Stop without starting the next task.

## Progress Log

- Started: created this execution plan before inspecting task details.
- Identified first incomplete task from `TODO.md`: `NT3` (`id handle 包装 + 错误映射（B.3 / B.4）`), with details in `TODO-1.md`.
- Reviewed `TODO-1.md` and `PLAN-1.md` for NT3 requirements; latest commit is `[NR2] Review serde conversion layer`, with no directly relevant unfinished issue noted.
- Current implementation plan for NT3: add `ids.rs` handle maps for `CallbackId` and `WindowId`, add `error.rs` conversions into `napi::Error`, update Node conversion helpers to use string callback handles, then validate and mark NT3 done.
- Implemented the initial NT3 code changes: added Node id handle maps, added error conversion helpers, and changed callback ids in conversion helpers/tests to use string handles instead of numeric ids.
- Validation passed: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --all --all-targets`.
- Marked `NT3` as `[DONE]` in `TODO-1.md`, updated the `TODO.md` index status to `DONE`, and added the NT3 completion record.
- Pre-commit review found unrelated untracked files `notification.sh` and `run_agent.sh`; they will not be staged.
