# Task execution plan

I will not record private chain-of-thought here. This file tracks the actionable plan, decisions, and progress for the current invocation.

## Plan

1. Read `TODO.md` first and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the task's referenced files and tests, then implement the task as written without splitting it unless a concrete prerequisite blocks correct execution.
4. Validate with the required formatting, linting, and test commands for this repository.
5. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling in its completion record. Update `PLAN.md` only if phase-level sequencing changes.
6. Commit all task-related changes with a clear message and stop without starting the next task.

## Progress

- Created the execution plan file.
- Selected first incomplete task: `#4 桌面背景纹理`.
- Confirmed latest commit completed `#3b` and does not add unfinished work relevant to `#4`.
- Updated `Desktop::draw` to fill the desktop background with `░`.
- Added `pty_desktop_background_uses_texture` to assert an uncovered desktop cell uses the texture glyph.
- Fixed PTY coordinate helpers that treated UTF-8 byte offsets as terminal columns; the new textured background exposed this in rich artifact/editor tests and similar helpers.
- Ran `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, focused PTY tests for touched helpers, and `cargo test --all --all-targets` successfully.
- Marked `#4` as `[DONE]` in `TODO.md` with completion notes.

## Task-specific plan

1. Change `Desktop::draw` so the frame background fill uses `░` instead of a plain space.
2. Add a focused PTY regression in `tests/pty_desktop.rs` asserting that an uncovered desktop work-area cell renders `░`.
3. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and the relevant/full test commands.
4. Mark `#4` as `[DONE]` in `TODO.md` with validation notes.
5. Commit the task changes and stop.

## New Invocation Plan
I will follow TODO.md as the source of truth, identify the first task whose heading is not prefixed with [DONE], implement exactly that task, run the required validation, update TODO.md with the completion record and [DONE] prefix, commit the resulting changes, and stop without advancing to the next task.

## Selected Task
- First incomplete TODO task: #5 菜单条整体化.
- Target: src/app/menu/draw.rs MenuBar::draw should fill the full menu row with the menu_bar style before drawing individual menu items.
- Validation plan: run cargo fmt, cargo clippy --all-targets -- -D warnings, relevant menu/desktop tests, then cargo test --all --all-targets.

## Progress
- Implemented MenuBar::draw full-row fill with theme.menu_bar before rendering top-level menu titles.
- Added a unit regression test asserting the trailing menu row cell is styled as menu_bar.
- Adjusted regression assertion to compare menu_bar foreground/background because TestBackend normalizes underline color in cell styles.
- Ran `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, the focused menu regression, and `cargo test --all --all-targets` successfully.
- Marked `#5` as `[DONE]` in `TODO.md` with completion notes.
