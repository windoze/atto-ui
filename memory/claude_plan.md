# Execution Plan

I will follow `TODO.md` as the authoritative task source and complete exactly the first incomplete task.

1. Read `TODO.md` to identify the first heading that is not prefixed with `[DONE]`.
2. Review only the files and context needed for that task, including `PLAN.md` only if the task affects phase-level planning.
3. Check the latest commit message for any explicitly unfinished issue that is directly relevant to the selected task.
4. Implement the selected task without narrowing scope or using workaround behavior.
5. Run formatting, linting, and relevant tests in the required order; address any unscheduled failures by fixing them or adding prerequisite tasks to `TODO.md`.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and adding a completion record. Update `PLAN.md` only if phase-level sequencing or criteria changed.
7. Commit all changes for this task with a clear message and the required co-author trailer, then stop.

## Current Task

Selected first incomplete task: `T28 — 更新测试 fixture 与 mock LSP 覆盖矩阵` from `TODO-2.md`.

Implementation focus:
- Add deterministic mock LSP responses for missing symbol methods (`textDocument/documentSymbol`, `workspace/symbol`) and their empty/error variants.
- Strengthen direct editor LSP tests so fixture coverage includes success and empty/error paths for symbol requests, plus missing empty/error paths for existing mock-backed LSP methods where practical.
- Keep PTY behavior deterministic and avoid timing sleeps beyond existing polling helpers.

Progress:
- Implemented mock LSP fixture extensions and direct editor LSP coverage.
- Ran `cargo fmt`, `cargo test -p atto-ui-editor --test lsp_editor`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all --all-targets` successfully.
- Marked T28 as `[DONE]` in `TODO.md` and `TODO-2.md` with a completion record.
