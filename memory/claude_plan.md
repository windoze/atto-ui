# Execution Plan

I will follow the task order in `TODO.md` and complete only the first task whose heading is not prefixed with `[DONE]`. This file records the actionable plan and progress updates; it intentionally avoids private reasoning details.

## Selected task

First incomplete task from `TODO.md`: **M1.2 运行状态查询接口**.

## Plan

1. Inspect `crates/atto-ui-terminal/src/terminal.rs` for `TerminalHandle`, `TerminalShared`, and the M1.1 exit-status state.
2. Add `TerminalHandle::is_running()` and `TerminalHandle::exit_status()` using the existing shared lifecycle state.
3. Ensure status polling observes already-exited children without requiring a draw, if the current architecture supports that safely.
4. Add focused tests covering running state before and after subprocess exit, exit status exposure, and no-subprocess behavior if applicable.
5. Run validation in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace --all-targets`.
6. Mark M1.2 as `[DONE]` in `TODO.md` with a completion record.
7. Commit all task-related changes and stop.

## Progress

- Initialized the current invocation plan tracker.
- Identified M1.2 as the first incomplete task.
- Latest commit is M1.1 and does not mention unfinished work that changes M1.2 ordering.
- Added shared lifecycle state for subprocess running status.
- Exposed `TerminalHandle::is_running()` and `TerminalHandle::exit_status()`.
- Added a focused process lifecycle test for handle running/exit-status polling.
- Focused `process_exit` tests passed.
- Full required validation sequence passed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Marked M1.2 as `[DONE]` in `TODO.md` with completion notes.
- Next step: commit the task changes and stop.
