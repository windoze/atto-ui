# Execution Plan

- Read TODO.md to identify the first task whose heading is not prefixed with [DONE].
- Check recent commit context only if it explicitly references unfinished work directly relevant to that task.
- Inspect the code and tests needed for that task, without broad unrelated triage.
- Implement the task as specified, avoiding workaround behavior and preserving existing conventions.
- Run formatting, clippy with warnings denied, and the relevant/full test suite required by the task.
- Update TODO.md completion status and record validation results; update PLAN.md only if phase-level sequencing changed.
- Commit all changes for this single completed task, then stop.

## Current task

First incomplete task: `T18 — L3 Rename UI 与跨已打开文件 WorkspaceEdit 应用`.

Task scope:
- Add `EditorAction::LspRename` with default `F2`.
- Request prepare-rename at the active editor position, then show rename input with a default name from the prepare range or current word.
- Submit rename on Enter, cancel on Esc.
- Apply returned `WorkspaceEdit` through the shared workspace LSP bridge to all opened buffers, report skipped unopened URIs, and refresh tab bindings/dirty state.
- Add tests for single-file rename, cross-open-file rename, and skipped unopened URI behavior.

## Implementation approach

- Extend `atto-ui-editor` with a non-modal rename popup model/window, local key handling, pending prepare/rename request state, and `EditorEvent::LspRenameWorkspaceEdit`.
- Keep actual multi-file edit application in `atto-editor-app`: consume the rename event, call `WorkspaceState::apply_workspace_edit`, and set a clear status message for applied/skipped files.
- Reuse the existing `WorkspaceState` binding synchronization so open tabs update and dirty markers refresh through existing draw/title sync.
- Extend the mock LSP server and tests for prepare/rename, emitted workspace edits, cross-open-file application, and skipped unopened URIs.

## Completion

- Implemented T18 rename UI and workspace edit application.
- Updated `TODO.md` and `TODO-2.md` to mark T18 `[DONE]` with a completion record.
- Validation passed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted rename tests, and `cargo test --workspace --all-targets`.
