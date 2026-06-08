# Claude Execution Plan

## Objective
Complete exactly the first incomplete task listed in `TODO.md`, using `TODO.md` as the authoritative ordering and completion source, then commit and stop.

## Plan
1. Read `TODO.md` to identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for unfinished work directly relevant to that task.
3. Inspect the task requirements and the relevant implementation/tests.
4. Implement the task without broad unrelated triage or workarounds.
5. Run required formatting, linting, and tests in the requested order: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required.
6. If an unscheduled failing test or blocking implementation gap is found, either fix it if in scope or add the minimum prerequisite task to `TODO.md`, commit that bookkeeping, and stop.
7. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record.
8. Commit all task-related changes with a descriptive message and the required co-authored trailer.
9. Stop without starting the next task.

## Progress
- Plan file created before task execution.
- Selected first incomplete task: `T21 — L6 Inlay Hints 与 composed grid 渲染` from `TODO-2.md`.
- Latest commit (`[R20] Review formatting failure paths`) is directly prior review work and does not add a separate unfinished prerequisite for T21.
- Next step: inspect the existing editor config, keymap, LSP controller, render path, mock LSP fixtures, and available `editor-core` inlay/composed APIs before editing.
- Implemented the first pass of T21: public inlay-hints config, `LspToggleInlayHints`, LSP inlay request/response handling, composed-grid rendering when enabled, inlay/code-lens theme styles, mock LSP response, snapshot mode, and direct/PTY tests.
- Ran `cargo fmt` successfully.
- Next step: run focused inlay-related tests, then lint and full workspace validation.
- Validation completed successfully after the final request-throttle/cursor edge fixes: focused direct/PTY inlay tests, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Marked `T21` as `[DONE]` in `TODO.md` and `TODO-2.md`, including a completion record.
- Next step: review final git diff, then commit the T21 changes and stop.
