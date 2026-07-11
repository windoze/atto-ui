# Execution Plan

I will keep this file updated with a concise execution plan and progress notes. Private reasoning is not recorded here, but the actionable plan and key decisions are.

1. Read `TODO.md` to identify the first task whose heading is not prefixed with `[DONE]`.
2. Inspect only the files needed to understand that task, plus the latest commit if it directly mentions an unfinished issue relevant to the selected task.
3. Implement the selected task completely, adding or updating tests and documentation that are directly required by the task.
4. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite required by `TODO.md`.
5. If an unscheduled failure or blocker appears, fix it if it is in scope; otherwise add the minimum prerequisite task to `TODO.md`, commit that bookkeeping, and stop.
6. When the selected task is complete, mark its `TODO.md` heading with `[DONE]`, update its completion record, commit all task-related changes, and stop.

Progress:
- Created the execution plan before reading project task details.
- Selected first incomplete task: `M5.1 第 1 层 感知与信号【组件层】`.
- Task objective: use vt100 callbacks to observe OSC 133/7 through `unhandled_osc`, maintain component-layer command block state, and avoid shell-integration hard dependencies.
- Next steps:
  1. Check the latest commit and worktree status for relevant unfinished work.
  2. Inspect `atto-ui-terminal` callback wiring, shared state, handle APIs, and tests.
  3. Add a command-mark state machine for OSC 133 prompt/command/output/end markers and OSC 7 cwd markers.
  4. Add targeted tests for OSC parsing and state transitions.
  5. Run formatting, clippy, and tests; then mark `M5.1` done and commit.
- Latest commit did not mention an unfinished issue relevant to M5.1; worktree only had this plan file before implementation.
- Implemented OSC 133/7 callback capture through `unhandled_osc`, internal command block state, cwd tracking, and focused unit coverage. Targeted `cargo test -p atto-ui-terminal osc133 -- --nocapture` passes after `cargo fmt --all`.
- Validation completed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` passed.
- Marked `M5.1` as `[DONE]` in `TODO.md`; next step is committing the task changes and stopping.
