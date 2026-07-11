# Claude Execution Plan

## Scope

- Source of truth: `TODO.md`.
- Goal for this invocation: complete exactly the first incomplete task in `TODO.md`, then stop.
- Completion rule: a task is complete only if its title is explicitly prefixed with `[DONE]`.
- I will not perform broad historical triage before selecting the current task.

## Execution Plan

1. Read `TODO.md` and identify the first task whose title is not prefixed with `[DONE]`.
2. Check recent git context only as needed for the selected task, including whether the latest commit mentions an unfinished issue directly relevant to it.
3. Inspect the code and tests relevant to that task.
4. Implement the task as written, without narrowing scope or adding workarounds.
5. Run required validation in order: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task and repository rules.
6. If validation reveals an unscheduled failing test or a blocking spec mismatch, fix it if it is in scope; otherwise add the minimum prerequisite task to `TODO.md`, keep the current task incomplete, commit that bookkeeping, and stop.
7. When the selected task is complete, update `TODO.md` by prefixing the task title with `[DONE]` and adding/updating its completion record.
8. Update this file after key steps or if the plan changes.
9. Review `git status`, `git diff`, and recent commits before committing.
10. Commit all changes for this task with a clear task-specific commit message.
11. Stop after the commit and do not proceed to the next task.

## Progress Log

- New invocation started. I will re-read `TODO.md`, select the first task whose title is not prefixed with `[DONE]`, and complete only that task before stopping.
- Selected first incomplete task: `M4.6 剪贴板（后续，可选）` from `TODO.md`. The implementation goal is to add system clipboard support via `arboard` and wire OSC 52 clipboard callbacks with OSC 52 taking priority over `arboard` fallback.
- Implemented terminal system clipboard wiring: default copy backend now emits OSC52 first and tries `arboard`, selection/copy-mode copies sync to the configured system clipboard, and valid OSC52 clipboard callbacks update both the local copy buffer and configured system clipboard. Focused terminal clipboard tests pass.
- Required validation passed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`. `TODO.md` is updated to mark `M4.6` as `[DONE]`.
- Initialized plan before reading project task files or running commands.
- Selected first incomplete task: `M4.5 alt screen 滚动分流` from `TODO.md`.
- Latest commit is `[M4.4] Implement terminal local clipboard`; it does not explicitly mention an unfinished issue that changes the selected task ordering.
- Relevant implementation path found in `crates/atto-ui-terminal/src/terminal.rs`: captured mouse events currently forward protocol mouse events first, then fall back to local scrollback. The missing branch is `alternate_screen()` wheel-to-arrow dispatch before local scrollback.
- Test strategy: add focused `input_encoding` coverage for alternate-screen wheel up/down, mouse protocol priority over alternate-screen, and main-screen scrollback remaining local.
- Implemented `handle_alternate_screen_wheel` and inserted it between mouse protocol forwarding and local scrollback fallback in the captured mouse event path.
- Added `input_encoding` tests for alternate-screen wheel-to-arrow behavior, mouse reporting priority, and main-screen local scrollback. `cargo test -p atto-ui-terminal --test input_encoding -- --nocapture` passed.
- Formal validation passed: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Updated `TODO.md` to mark `M4.5` as `[DONE]` with completion and validation records.
