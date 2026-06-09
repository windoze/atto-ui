# Claude Execution Plan

## Current task

- First incomplete `TODO.md` task: `#1 窗口标题居中`.
- Scope: update window chrome title rendering so window titles are centered on the top border with one blank cell of padding before and after the title, then add/update snapshot coverage.

## Step-by-step plan

1. Review the latest commit message for any unfinished issue directly relevant to centered window titles.
2. Inspect the window chrome rendering code and existing PTY/snapshot tests that cover window borders and titles.
3. Implement centered title placement in `src/wm/manager/chrome.rs`, preserving border corners, controls, clipping, focus styles, and Unicode-width behavior.
4. Add or update regression coverage proving titles are centered with one blank cell on each side.
5. Run formatting, clippy with warnings denied, then the relevant and full test suites required by the task.
6. Update `TODO.md` by prefixing task `#1` with `[DONE]` and adding a completion note with validation results.
7. Commit all task-related changes with a descriptive message and stop.

## Progress

- Identified first incomplete task from `TODO.md`.
- Reviewed latest commit; it introduces the UI alignment plan and does not mention a separate unfinished title-centering blocker.
- Implemented centered, space-padded fallback window title drawing.
- Added a PTY regression that asserts the initial `Widgets` title is surrounded by blank padding at its centered title position.
- Full test suite exposed an off-by-one in the new centered-title draw bound; corrected it so the final grapheme renders before trailing padding.
- Validation passed: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, affected PTY tests, and `cargo test --all --all-targets`.
- Marked TODO task `#1` as `[DONE]` with a completion record.
