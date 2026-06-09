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

- Initial execution plan confirmed.
- First incomplete task selected: `#13 系统菜单图标` — add a `≡` system-menu icon on the left side of the menu bar.
- Latest commit references completed `#12` work and does not introduce unfinished work directly relevant to `#13`.
- Implemented the menu-bar icon as a themed `system-menu-icon` glyph with a one-column shared title inset across drawing, dropdown anchoring, and mouse hit testing.
- Added unit coverage for icon rendering, shifted Unicode/mnemonic layout, mouse title hit testing, and the default theme glyph.
- Validation completed successfully with `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test -p atto-ui system_menu --lib`, and `cargo test --all --all-targets`; no `tools/run_fixtures.py` fixture runner exists.
- Marked `#13` as `[DONE]` in `TODO.md` with its completion record.
- Committed task-related changes as `b1cfdc7` (`[#13] Add system menu icon`).
- Stop after this task; the next invocation should start from the next incomplete `TODO.md` item.
