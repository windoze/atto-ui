## Execution Plan

1. Read `TODO.md` to identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for directly relevant unfinished work tied to that task.
3. Inspect the task's referenced code, tests, and documentation to understand its exact requirements.
4. Implement the task without changing unrelated behavior or adding workarounds.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant tests, escalating to the full suite when required.
6. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling in its completion record.
7. Update this plan file at key milestones, commit all task-related changes with the required trailer, and stop without starting the next task.

## Current Task

- First incomplete task: `M3.3 前缀命令表`.
- Latest commit: `[M3.2] Add configurable terminal prefix key`; directly relevant because M3.3 builds on the configurable prefix-key state machine from M3.2.

## Task-Specific Steps

1. Inspect the existing terminal prefix pending/fallback implementation and current event bubbling hooks.
2. Add a prefix command table covering `prefix+F10`, `prefix+w`, `prefix+z`, `prefix+[`, and `prefix+prefix`.
3. Preserve lossless fallback for non-command keys and route literal prefix escape to the child process.
4. Add or update targeted tests for command recognition and literal prefix forwarding.
5. Run formatting, linting, targeted tests, and required full validation before marking `TODO.md`.

## Progress

- Implemented the prefix command table in `TerminalEmulator`.
- Added copy-mode placeholder state and literal-prefix escape dispatch.
- Added component actions so focused terminal views can activate menu, toggle window management, and maximize/restore their own window.
- Added unit coverage for command mapping, copy-mode placeholder, and literal prefix escape.
- Added PTY coverage for `prefix+F10`, `prefix+w`, and `prefix+z` driving desktop chrome.
- Validation passed: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted prefix/PTY/desktop-action tests, and `cargo test --workspace --all-targets`.
- Marked only `M3.3 前缀命令表` as `[DONE]` in `TODO.md`.
- Updated `PLAN.md`, `TERMINAL_GAP.md`, and the next TODO entry to reflect the typed `ComponentAction` dispatch bridge used instead of raw-key bubbling.
- No additional test rerun was needed after these final Markdown-only documentation updates.
