# Execution Plan

I will keep this file updated as a concise execution plan and progress log for the current invocation. I will not record private chain-of-thought, but I will record decisions, key observations, and completed steps so progress is auditable.

## Steps

1. Read `TODO.md` first and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the selected task requirements, dependencies, and validation instructions.
4. Implement the selected task completely, or if a concrete blocker prevents correct implementation, add the minimum prerequisite task in `TODO.md` and stop after committing that bookkeeping.
5. Run required formatting, linting, and tests in the requested order: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required.
6. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling its completion record. Update `PLAN.md` only if phase-level sequencing or criteria change.
7. Commit all changes for this task with a clear task-specific message, then stop without starting the next task.

## Progress

- Invocation started.
- Initial plan written before inspecting project files.
- Read `TODO.md`; first incomplete task is `M6.3 spawn 环境`.
- Scope for this invocation: set subprocess `TERM` / `COLORTERM`, apply initial cwd for spawned sessions/commands, add an explicit terminal resize API so resize is not only performed from `draw`, validate, update `TODO.md`, commit, and stop.
- Implemented spawn preparation (`TERM`, `COLORTERM`, default cwd), shared PTY resize plumbing, `TerminalEmulator::resize`, and `TerminalHandle::resize`.
- Added tests for command preparation, actual spawn environment/cwd/initial size, and live PTY resize.
- Validation passed: `cargo fmt --all`, targeted `atto-ui-terminal` spawn/process tests, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Updated `TODO.md` to mark `M6.3` as `[DONE]` with completion and validation notes. No `PLAN.md` change was needed because phase-level sequencing did not change.
