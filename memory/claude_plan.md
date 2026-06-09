# Execution Plan

## Objective
Complete exactly the first incomplete task listed in `TODO.md`, then stop after documenting completion and committing the result.

## Constraints
- `TODO.md` is the source of truth for task ordering and completion state.
- A task is complete only if its title is prefixed with `[DONE]`.
- Do not proceed to the next task after finishing the first incomplete task.
- Do not use workarounds for missing features, broken tests, or spec mismatches; fix them if in scope or add the minimum prerequisite task in `TODO.md` and stop.
- Update `PLAN.md` only if the phase-level plan changes.
- Commit all task-related changes before stopping.

## Steps
1. Read `TODO.md` and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the files and tests relevant to the selected task.
4. Implement the task completely, or add a prerequisite task if a concrete blocker makes correct implementation impossible.
5. Run formatting, linting, and relevant tests in the required order; run the full test suite when code changes require it.
6. Update `TODO.md` with `[DONE]` and a completion record when the selected task is finished.
7. Commit the complete task state with a clear message and required co-author trailer.
8. Stop without starting another task.

## Progress Log
- Initial plan recorded before task execution.
- Selected first incomplete task from `TODO.md`: `#6 点击只高亮、无下沉阴影`.
- Latest commit is `#5` and does not mention unfinished work directly relevant to task `#6`.
- Inspected menu drawing: top-level menu titles already select `theme.menu_bar_active` when active; `draw_shadow` is only used for dropdown panels. Next step is to add focused regression coverage and document completion.
- Added a focused unit test that isolates an active top-level menu title, verifies its cells use `menu_bar_active`, keeps the following separator on `menu_bar`, and confirms no title shadow is drawn below the title.
- Validation passed: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all --all-targets`.
- Marked `#6` as `[DONE]` in `TODO.md` with the completion record.
