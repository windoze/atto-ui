# Execution Plan

I will complete exactly the first incomplete task from `TODO.md` and then stop. I will not perform broad triage before selecting that task.

1. Read `TODO.md` to identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that task.
3. Inspect the task requirements, dependencies, and validation instructions.
4. Implement the task as written, adding only concrete prerequisite tasks to `TODO.md` if the task is blocked by an unscheduled implementation or test issue.
5. Run formatting, linting, and relevant tests in the required order, escalating to the full suite when required by the task or code changes.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling in its completion record. Update `PLAN.md` only if phase-level sequencing changes.
7. Commit all task-related changes with a descriptive message and the required co-author trailer.
8. Stop without starting the next task.

## Progress

- Initial execution plan confirmed for this invocation.
- First incomplete task selected: `#14 顶层菜单项间距` — adjust spacing between top-level menu items based on the current visual/theme direction.
- Latest commit does not introduce unfinished work directly relevant to `#14`.
- Implemented compact top-level menu spacing by removing the extra neutral gap between padded menu titles through shared layout helpers.
- Updated drawing, dropdown anchoring, and mouse hit testing to use the same title-width/next-title helpers so visual positions and interactions remain aligned.
- Added/updated unit tests for compact rendered spacing, title positions, active-title boundary styling, and mouse hit testing.
- Validation completed successfully with `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all --all-targets`; no `tools/run_fixtures.py` fixture runner exists.
- Marked `#14` as `[DONE]` in `TODO.md` with its completion record.
- Next steps: commit relevant tracked changes and stop.
