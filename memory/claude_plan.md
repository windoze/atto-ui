# Execution Plan

I will follow the task order in `TODO.md` and complete only the first task whose heading is not prefixed with `[DONE]`. This file records the actionable plan and progress updates; it intentionally avoids private reasoning details.

## Selected task

First incomplete task from `TODO.md`: **M1.3 new_with_callbacks 改造**.

## Plan

1. Inspect `crates/atto-ui-terminal/src/terminal.rs` and related tests to understand the current vt100 parser, shared state, handle API, and callback patterns.
2. Check the latest commit for directly relevant unfinished M1.3 notes.
3. Replace the bare `Parser::new` construction with `Parser::new_with_callbacks`.
4. Bridge vt100 callbacks for window title, icon name, audible bell, and clipboard copy into `TerminalShared`.
5. Expose callback-observed state through `TerminalHandle` and/or user callbacks following existing lifecycle API conventions.
6. Add focused tests proving title, icon name, bell, and clipboard callback events are observable.
7. Run validation in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace --all-targets`.
8. Mark M1.3 as `[DONE]` in `TODO.md` with a completion record.
9. Commit all task-related changes and stop.

## Progress

- Updated this invocation to target M1.3 after reading the current `TODO.md`.
- Latest commit is M1.2 and does not mention an unfinished blocker for M1.3.
- Implemented the callback-enabled vt100 parser bridge for title, icon, audible bell, and OSC 52 clipboard-copy events.
- Added handle accessors and public callback registration methods for the new callback-observed state.
- Added focused terminal callback tests.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` passed after formatting.
- `cargo test --workspace --all-targets` passed.
- Marked M1.3 as `[DONE]` in `TODO.md` with completion notes.
- Next step: commit all task-related changes and stop.
