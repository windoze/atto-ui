# Execution Plan

I cannot record private chain-of-thought, but I will keep this file updated with the actionable plan, progress, decisions, blockers, and validation results.

## Plan

1. Read `TODO.md` first and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit only for explicitly unfinished work that is directly relevant to that selected task.
3. Inspect the files and tests needed for that task, without doing broad unrelated triage.
4. Implement the task completely, preserving existing behavior outside the task scope.
5. Run formatting, linting, and relevant tests in the required order.
6. Update `TODO.md` to prefix the completed task heading with `[DONE]` and add a completion record.
7. Commit all task-related changes with a descriptive message and stop without starting the next task.

## Progress

- Created this plan file before task execution.
- Identified first incomplete task: `R28 — 审阅 T28`.
- Reviewed the T28 commit and found a blocking coverage gap for the R28 acceptance matrix: hover, semantic tokens, and folding ranges need empty/error fixture coverage.
- Updated execution plan: add deterministic mock fixture branches and tests for those gaps, then rerun required validation before completing `R28`.
- Implemented the missing mock LSP empty/error branches for hover, semantic tokens, and folding ranges.
- Added coverage for the new branches, including JSON-RPC framing checks and hover stale-popup clearing behavior.
- Focused validation passed: `cargo test -p atto-ui-editor --test lsp_editor`.
- Full validation passed: `cargo fmt`; `cargo clippy --all-targets -- -D warnings`; `cargo test --all --all-targets`.
- Marked `R28` as `[DONE]` in `TODO.md` and `TODO-2.md` with a completion record.
