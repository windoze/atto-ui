## Execution Plan

I will not record private chain-of-thought here, but this file will track the concrete plan, assumptions, progress, and key decisions for the current invocation.

1. Read `TODO.md` first and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for directly relevant unfinished work tied to that task.
3. Inspect the task requirements, dependencies, validation requirements, and any completion-record expectations.
4. Implement the task exactly as specified, avoiding scope narrowing or workaround behavior.
5. Run the required formatting, linting, and tests in the requested order, escalating to the full suite only when needed by the task and current changes.
6. If a concrete blocker or unscheduled failing test prevents completion, update `TODO.md` with the minimum prerequisite task(s), commit that bookkeeping, and stop.
7. If the task is completed, update `TODO.md` by prefixing the task heading with `[DONE]` and filling in the completion record.
8. Commit all relevant changes with a clear task-specific message and stop without starting the next task.

## Progress

- Created/updated this execution plan before running repository commands.
- Read `TODO.md` and identified the first incomplete task as `M5.6 测试`: add/verify unit tests for OSC 133/7 parsing, `command_blocks()` state, unmarked fallback, and PTY coverage for layer-2 navigation plus command-level copy where implemented.
- Added focused test coverage for multi-block OSC 133/7 parsing, unknown marker fallback, command-block action degradation without markers, and PTY Ctrl+Up/Ctrl+Down command navigation.
- Fixed the PTY navigation test expectation after verifying Ctrl+Up moves to the nearest prior command anchor; the test now uses two Ctrl+Up presses to reveal the previous off-screen command block and one Ctrl+Down to return.
- Completed validation with `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`; marked `M5.6` as `[DONE]` in `TODO.md` with completion notes.
