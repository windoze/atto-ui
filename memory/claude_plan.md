# Execution Plan

I will follow the task list exactly and complete only the first incomplete task in `TODO.md`.

1. Read `TODO.md` and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Read the selected task's requirements, dependencies, and validation instructions.
4. Inspect the relevant code and tests needed for that task.
5. Implement the task completely, avoiding unrelated changes and workarounds.
6. Run formatting, linting, and the required tests in the requested order.
7. If validation reveals an unscheduled failing test or blocker, fix it if in scope or add the minimum prerequisite task to `TODO.md`, commit, and stop.
8. If successful, mark the task heading in `TODO.md` with `[DONE]`, update its completion record, commit all changes for this task, and stop.

## Current Task

Selected first incomplete task: **M5.2 第 1 层 查询接口**.

Implementation focus:

1. Expose command block query data from `TerminalHandle`.
2. Expose `last_exit_code()` for the most recently finished OSC 133 command block.
3. Add an optional `on_command_finished(status)` callback if it fits the existing callback style.
4. Preserve safe degraded behavior when no OSC markers are present.
5. Add focused tests for the new handle APIs and callback behavior, then run the required Rust validation sequence.

## Progress

- Identified M5.2 as the first incomplete task.
- Added public `TerminalCommandBlock` snapshots, `TerminalHandle::command_blocks()`, `TerminalHandle::last_exit_code()`, and `TerminalEmulator::on_command_finished(...)`.
- Added focused callback/query tests, including no-marker degradation.
- Completed `cargo fmt --all` and focused `atto-ui-terminal` callback tests successfully.
- Fixed the clippy `question_mark` warning in the OSC 133 marker parser.
- Completed `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` successfully.
- Completed `cargo test --workspace --all-targets` successfully under the 30-minute cap.
- Marked M5.2 as `[DONE]` in `TODO.md` with a completion record and validation log.
