# M7.6 execution plan

Current task source: `TODO.md`.

First incomplete task: `M7.6 测试` - add PTY coverage for opening the terminal configuration UI, changing scrollback/prefix key/color palette and applying those changes, verifying persistence after restart, and checking cursor shape rendering from vt100 sequences.

Latest commit context: `a33d29e [M7.5] Wire terminal config into runtime`, directly relevant because M7.6 validates the runtime configuration wiring added there.

Planned steps:
1. Inspect the M7 configuration implementation, terminal viewer fixtures, and existing PTY tests to find the intended UI controls, status probes, and test helpers.
2. Add targeted PTY tests that exercise the already-implemented config window through user-visible paths rather than internal shortcuts where feasible.
3. If testability gaps block the specified checks, add minimal fixture/status hooks consistent with existing test patterns instead of changing production behavior.
4. Run `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, then targeted PTY tests, and finally `cargo test --workspace --all-targets` because M7.6 is a test/code task.
5. Update `TODO.md` by prefixing M7.6 with `[DONE]` and recording implementation/validation details.
6. Commit all changes for M7.6 with a clear message and the required co-author trailer, then stop.

Progress:
- Created this plan after identifying M7.6 as the first incomplete task.
- Inspected the M7 settings view, snapshot terminal window app, PTY host helpers, and existing PTY tests.
- Planned a fixture-only status line that reports live terminal scrollback/prefix/cursor plus persisted ANSI palette, and PTY cell style helpers for cursor rendering assertions.
- Implemented PTY host cell style helpers, snapshot fixture config status reporting, and new M7.6 PTY tests for settings apply/save/reload plus cursor shape rendering.
- Fixed the initial test-driving issues by scrolling to the actual ANSI palette controls and recapturing the terminal before probing live palette behavior after settings close.
- Targeted M7.6 PTY tests now pass.
- Fixed a 24-row fixture regression by making the extra config status line visible only when the terminal window fixture has enough vertical space; the full `pty_terminal_window_interactions` test file now passes.
- Full validation completed successfully with formatting check, workspace clippy, and full workspace tests.
- Marked M7.6 as `[DONE]` in `TODO.md` with completion and validation notes.
