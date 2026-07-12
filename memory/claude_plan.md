# Claude execution plan

I cannot provide private chain-of-thought, but this file records the actionable execution plan and progress for this invocation.

## Current status

- Selected first incomplete task from `TODO.md`: **示例升级**.
- Latest commit is `[Docs] Update terminal app documentation`; it does not mention unfinished work that changes this task's scope.
- Current direction: upgrade `crates/atto-ui-terminal/examples/terminal_viewer.rs` so the demo visibly showcases the already-implemented full terminal app features, rather than leaving those features discoverable only through hidden shortcuts.

## Step-by-step plan

1. Read the existing `terminal_viewer` example and nearby terminal window fixture patterns.
2. Add an explicit full-feature guide surface to the demo UI, including prefix-key commands, copy-mode, split panes, session management, command-block actions, and settings/config persistence.
3. Ensure the startup banner also advertises the full feature set from inside the terminal pane.
4. Build/check the example with the smallest relevant validation, then run formatting, clippy, and workspace tests if code changed.
5. Mark **示例升级** as `[DONE]` in `TODO.md` with a completion record.
6. Commit the task changes with the required co-author trailer and stop.

## Progress

- Identified **示例升级** as the first incomplete task.
- Confirmed the latest commit has no explicit unfinished issue relevant to this task.
- Reviewed the current `terminal_viewer` implementation; it already wires sessions, splits, copy-mode, command context menus, and settings, so the missing piece is making the demo explicitly present those full-feature paths.
- Updated `terminal_viewer` with a startup/reopenable feature guide, a Help menu entry, a richer in-terminal banner, a distinct default demo command profile for `File > New command window`, and duplicate-prevention for settings/guide windows.
- Ran `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`; all passed.
- Marked **示例升级** as `[DONE]` in `TODO.md` with the completion and validation record.
