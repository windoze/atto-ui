# Claude Execution Plan

## Scope
- Follow `TODO.md` as the authoritative task source.
- Identify the first task whose heading is not prefixed with `[DONE]`.
- Complete exactly that task, validate it, update task records, commit, and stop.

## Execution Steps
1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Inspect the latest commit only if it mentions an unfinished issue directly relevant to that task.
3. Read only the code and docs needed to understand and implement the selected task.
4. Implement the task with small, targeted patches.
5. Update this plan file when key steps are completed or if the plan changes.
6. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the required test suite unless the task is documentation-only and a previous green run can be reused.
7. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling its completion record.
8. Review `git status`, `git diff`, and recent commits, then commit all relevant changes with a descriptive message.
9. Stop without starting the next task.

## Progress
- Initial plan file created.
- Selected first incomplete task: `T6 — 任务取消抽象（core，std-only）（C.1）`.
- Next step: inspect the existing AppHost/event-loop paths and test fixtures needed to add std-only cancellation, task registry, running-state property, and Esc interruption coverage.
- Implemented the initial T6 code path: added `src/task/` with `CancellationToken`, `TaskHandle`, `TaskRegistry`, running `Property<bool>`, AppHost/event-loop Esc cancellation integration, and a PTY fixture/test for cancellable std-thread work.
- Next step: run formatting and targeted tests to catch compile/lint issues before full validation.
- Validation completed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted task/AppHost/PTY tests, full `cargo test --workspace --all-targets`, and `cargo tree -p atto-ui` dependency inspection all passed; the core dependency tree still has no tokio.
- Next step: mark T6 `[DONE]` in `TODO.md`, record completion details, review the diff/status, and commit only the T6-related files.
- `TODO.md` now marks T6 as `[DONE]` with the implementation and validation record.
- Next step: inspect `git status`, `git diff`, and recent commits, then stage and commit only T6-related files.
