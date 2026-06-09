# Execution Plan

Note: I will record actionable plans, decisions, and progress here rather than private chain-of-thought.

1. Read TODO.md to identify the first incomplete task, treating only headings prefixed with [DONE] as complete.
2. Review the selected task details, dependencies, validation requirements, and the latest commit for directly relevant unfinished work.
3. Inspect the relevant code and tests needed for that single task only.
4. Implement the task completely or, if blocked by a concrete prerequisite, update TODO.md with the minimum prerequisite task and stop.
5. Run formatting, clippy with warnings denied, and the relevant/full test suite as required by TODO.md and the repository instructions.
6. Update TODO.md completion record and [DONE] prefix for the completed task; update PLAN.md only if phase-level sequencing changes.
7. Commit all task-related changes with a clear message and stop without starting the next task.

## Selected Task

First incomplete task in `TODO.md`: `#3b 按钮尺寸回归`.

## Task-Specific Plan

1. Change `Button`'s intrinsic layout height from 3 rows to 1 row while preserving its flat one-line rendering and shadow behavior.
2. Update button unit tests so they verify the new 1-row layout contract and click hit area.
3. Update deterministic fixtures/manual layouts that still reserve 3 rows for button-only rows, especially `snapshot_app` T19, and adjust PTY click coordinates if the visual rows move.
4. Update affected examples/demos that explicitly used 3-row button toolbar heights.
5. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and the relevant/full test suite required for completion.
6. Mark `#3b` `[DONE]` in `TODO.md`, record validation, commit the changes, and stop.

## Progress

- Implemented the `Button` layout height change (`min_height` and `desired_height` now report 1).
- Updated button unit tests to draw and hit-test one-row button areas.
- Updated the T19 deterministic fixture to allocate one-row button rects and compact subsequent controls accordingly.
- Updated button-only demo toolbar rows that still reserved a fixed height of 3.
- Ran `cargo fmt` successfully.
- Ran `cargo clippy --all-targets -- -D warnings` successfully.
- Ran focused validation successfully: `cargo test -p atto-ui widgets::button --lib` and `cargo test -p atto-ui --test pty_core_widgets_t19`.
- Ran full validation successfully: `cargo test --all --all-targets`.
- Marked `#3b` as `[DONE]` in `TODO.md` with completion notes.
