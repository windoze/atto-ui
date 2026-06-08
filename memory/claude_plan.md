# Execution Plan

I will use TODO.md as the authoritative task list, identify the first task whose heading is not prefixed with [DONE], complete exactly that task, validate it according to its requirements, update TODO.md with a completion record and [DONE] prefix, commit the resulting changes, and stop.

Steps:
1. Read TODO.md first to find the first incomplete task and its validation requirements.
2. Check the latest commit only for unfinished work directly relevant to that selected task.
3. Inspect the minimal code and documentation needed for the selected task.
4. Implement the task without narrowing scope or using workarounds.
5. Run formatting, clippy with warnings denied, and the required tests in the requested order.
6. If an unscheduled blocking failure appears, fix it or add the minimum prerequisite task in TODO.md and stop after committing.
7. Mark the completed task title with [DONE], update its completion record, commit all task-related changes, and stop before starting any next task.

Progress:
- Selected first incomplete task: T20 — L5 Formatting manual formatting and format-on-save interface.
- Inspected `TODO-2.md`, `PLAN-2.md`, editor LSP/action/config code, app command/window save paths, and the `editor-core-lsp` formatting APIs.
- Implementation plan: add editor formatting config/action/state, request `textDocument/formatting` with current indentation options, apply returned edits as a single undoable batch, surface format completion/failure events, expose app `FormatActive` command and command-palette entry, add format-on-save plumbing behind the new config, extend the mock LSP, and add focused tests.
- Implemented T20 formatting support, added focused tests and mock LSP formatting responses, added working `Ctrl+K Ctrl+F` editor handling, ran formatting/clippy/full tests successfully, and marked T20 `[DONE]` in `TODO.md` / `TODO-2.md`.
- Next step: review git status and commit the task changes.
