# Execution Plan

I will work on exactly the first incomplete task listed in `TODO.md` and stop after it is completed, documented, tested, and committed.

## Steps

1. Inspect the current task list by reading `TODO.md`, then identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Read the selected task details, dependencies, validation requirements, and any nearby completion records.
4. Inspect only the code and documentation needed for that task.
5. Implement the requested change without splitting the task unless a concrete prerequisite makes completion impossible.
6. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the required tests, escalating to the full suite when needed.
7. If an unscheduled failing test or blocker is found, fix it if it is in scope; otherwise add the minimum prerequisite task to `TODO.md`, commit that bookkeeping, and stop.
8. When the task is complete, mark its `TODO.md` heading with `[DONE]` and update its completion record with the meaningful implementation and validation details.
9. Commit all task-related changes with a clear message and the required co-author trailer.
10. Stop after this one task.

## Progress

- Initial execution plan written before task inspection.
- Identified first incomplete task: `M1.4 测试`, covering process exit status/running/on_exit behavior and observable title/bell/clipboard callbacks.
- Existing tests cover direct parser callbacks and subprocess exit. I will add a spawned-shell callback regression test, then run the required validation sequence.
- Added the spawned-shell callback/exit regression test and marked `M1.4` complete in `TODO.md` after workspace validation passed.
- Validation completed: formatting, focused terminal tests, workspace clippy, and full workspace tests all passed.
