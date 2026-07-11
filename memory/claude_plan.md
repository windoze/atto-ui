# Execution Plan

I will follow the task order in `TODO.md` and complete only the first task whose heading is not prefixed with `[DONE]`. This file records the actionable plan and progress updates; it intentionally avoids private reasoning details.

## Selected task

First incomplete task from `TODO.md`: **M1.1 进程退出信号**.

## Plan

1. Inspect `crates/atto-ui-terminal/src/terminal.rs` around process spawning, reader shutdown, shared terminal state, and existing close/drop callbacks.
2. Identify the existing callback types and tests that should be extended for process exit reporting.
3. Add shared exit state that records the child `ExitStatus` exactly once when reader EOF or `try_wait()` observes process termination.
4. Add an `on_exit(status)` callback path distinct from the existing `on_close` path.
5. Wire process lifecycle code so exit detection updates shared state and invokes the callback without breaking capture, paste, scrollback, or normal drop behavior.
6. Add focused tests for M1.1 exit reporting and callback behavior.
7. Run validation in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace --all-targets`.
8. Mark M1.1 as `[DONE]` in `TODO.md` with a completion record.
9. Commit all task-related changes and stop.

## Progress

- Identified M1.1 as the first incomplete task.
- Replaced stale plan content from a prior invocation with the current task plan.
- Added shared process-exit state, idempotent exit recording, `on_exit(status)` callback wiring, reader EOF detection, and draw-time `try_wait()` polling.
- Added focused integration tests for subprocess exit callback delivery and `on_exit` versus `on_close` separation.
- Fixed the fast-exit case by adding a nonblocking process watcher thread that polls `try_wait()` until the child exits or the process is stopped.
- Focused `process_exit` integration tests now pass.
- Full required validation sequence passed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Marked M1.1 as `[DONE]` in `TODO.md` with completion notes.
- Next step: commit the task changes and stop.
