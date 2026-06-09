# Execution Plan

I will follow TODO.md as the authoritative task list and complete exactly the first task whose heading is not prefixed with `[DONE]`. I will not perform broad triage before selecting that task.

## Steps
1. Read TODO.md to identify the first incomplete task and its requirements, dependencies, validation instructions, and completion record expectations.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect the minimum relevant project files needed to implement the selected task correctly.
4. Implement the task as written, avoiding workarounds or scope narrowing.
5. Run formatting, clippy with warnings denied, and the relevant/full tests required by the task and repository policy.
6. If an unscheduled blocking issue or failing test is found, either fix it immediately or add the minimum prerequisite task to TODO.md before the blocked task, commit that bookkeeping, and stop.
7. When the task is complete, update TODO.md by prefixing the task heading with `[DONE]` and filling the completion record with implementation and validation notes.
8. Commit all task-related changes with a clear task-specific message and the required co-author trailer.
9. Stop after this single task.

## Progress Log
- Plan created before selecting or executing the current TODO task.
- Selected first incomplete task: `#10 新增 Theme::turbo()` in TODO.md. Scope is adding a Turbo Vision-style preset with blue desktop, gray/cyan menu and status bars, gray dialog/window background, green selection highlight, theme-file support, and no change to the default theme.
- Current implementation plan: add `Theme::turbo()` beside `dark()` and `light()` using existing named-style population; extend `ThemeConfig` with an optional base/preset selector so theme files can request `turbo`; keep `Theme::load_from_path` defaulting to `dark()` when no base is specified; add unit tests for the preset, config-file base selection, overlays, and unchanged defaults.
- Implemented draft changes: added `ThemeConfig.base`, `Theme::named()`, `Theme::turbo()`, theme-file base selection, Node/Python theme-name routing, TypeScript theme-name typing, demo menu exposure, README usage notes, and focused theme/Python smoke tests.
- Ran focused Rust theme tests with `cargo test -p atto-ui theme --lib`; all focused theme tests passed.
- Fixed a clippy build failure by restoring the Python binding's shared `normalize_name` helper for non-theme parsing, then reran `cargo clippy --workspace --all-targets -- -D warnings`; clippy passed.
- Full validation completed: `cargo test --all --all-targets`, `npm run typecheck --prefix packages/core`, `npm test --prefix packages/core`, `maturin develop` for the Python binding, and `python3 crates/atto-ui-python/tests/test_e2e.py` all passed. `pytest` was not installed, so the Python unittest suite was run directly as documented by the package README.
- Updated `TODO.md` to mark `#10` as `[DONE]` and added the completion record. Also updated the Python README to document `turbo`; this was documentation-only after validation.
