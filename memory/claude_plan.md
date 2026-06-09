# Claude Execution Plan

## Current objective
- Complete the first incomplete task in `TODO.md`: R27 — 审阅 T27.
- Treat `TODO.md` and `TODO-2.md` as the authoritative ordering and completion sources.
- Review only the T27 decision/documentation change set, because the latest commit is `[T27] Document workspace editor view decision`.

## Step-by-step plan
1. Read `TODO.md` and identify the first heading not prefixed with `[DONE]`.
2. Check the latest commit message for unfinished work directly relevant to that task.
3. Inspect only the task-related source files, tests, and documentation needed to implement the selected task correctly.
4. Implement the task as specified, or add the minimum prerequisite task to `TODO.md` if a concrete blocker prevents spec-correct completion.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite required by the task.
6. Update `TODO.md` with the `[DONE]` prefix and completion record when the task is complete; update `PLAN.md` only if phase-level sequencing changes.
7. Commit all task-related changes with a clear message and stop without starting the next task.

## R27 review plan
1. Verify T27 recorded a concrete architecture decision with the future type and file path.
2. Verify T27 did not introduce unused prototype/dead code.
3. Verify the decision gives future tasks a clear rule for staying on `EditorView` versus introducing `WorkspaceEditorView`.
4. If no blocking issue is found, mark R27 `[DONE]` in `TODO.md` and `TODO-2.md`, record validation scope, commit, and stop.

## R27 progress
- First incomplete task identified: R27 in `TODO-2.md`.
- Latest commit `[T27] Document workspace editor view decision` is directly relevant.
- T27 changed `PLAN-2.md`, `TODO-2.md`, `TODO.md`, `workspace_state.rs` module documentation, and `memory/claude_plan.md`; no production code or prototype implementation was added.
- Focused code-review pass found no blocking issues against the R27 criteria.
- R27 marked `[DONE]` in `TODO.md` and `TODO-2.md`; completion record documents the concrete decision, lack of dead code, future routing rule, and validation scope.
