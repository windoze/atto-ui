# Execution Plan

I will follow `TODO.md` as the authoritative task list and complete exactly the first task whose heading is not prefixed with `[DONE]`. I will not perform broad issue triage before selecting that task.

Steps:
1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit only for explicitly mentioned unfinished work that is directly relevant to that task.
3. Inspect the minimum relevant code, tests, and documentation needed to implement the selected task.
4. Implement the task without narrowing scope or introducing workarounds.
5. Run formatting, linting, and the required tests in the requested order, escalating to the full test suite when needed.
6. If an unscheduled failing test blocks completion, fix it or add the minimum prerequisite task before the current task in `TODO.md`, then stop.
7. Mark the completed task title with `[DONE]` in `TODO.md` and update its completion record.
8. Commit all changes for this invocation with a descriptive message and the required co-author trailer.
9. Stop without starting the next task.

Selected task: M3.4 "事件派发桥接".

Task-specific plan:
1. Completed: checked latest commit (`[M3.3] Add terminal prefix command table`) and confirmed M3.4 is the next task.
2. Completed: inspected `ComponentAction`, `EventResult`, terminal prefix command emission, desktop/window-manager dispatch paths, pointer capture, tooltip dispatch, and `send_event_to_window`.
3. Completed: found an existing uncommitted M3.4 implementation that routes typed component actions through a shared desktop bridge and blocks shell commands while a modal is active.
4. Completed: targeted non-modal bridge, modal bridge, and terminal prefix action tests passed.
5. Completed: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` passed.
6. Completed: marked M3.4 `[DONE]` in `TODO.md` with validation notes.
7. Completed: final diff reviewed; all current invocation changes are being committed for M3.4.
