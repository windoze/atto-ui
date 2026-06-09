# Execution Plan

## Reasoning summary
- `TODO.md` points to `TODO-2.md`; the first incomplete task is T24: file-tree drag move, clipboard, and git status refresh.
- Existing file-tree drag/drop hooks are no-op and explorer Cut/Copy/Paste are placeholders, so T24 can be implemented directly without adding prerequisite tasks.
- Drag-source detection happens before the file-tree receives the mouse-down event, so a drag payload must use the row under the pointer when it is not already selected.
- Explorer drops must validate payload ids against the current workspace tree and roots; no file operation should overwrite an existing destination or move/copy a directory into itself/descendant.
- Git status commands must run off the draw/event hot path; the view will cache results from a background worker and rebuild the tree when results are available.

## Step-by-step plan
1. Add a public file-tree drag payload type constant and implement `FileTree::drag_source_at` to emit selected/pointed node ids.
2. Extend `ExplorerWindowView` with clipboard state, git status cache/worker state, and helper methods for root validation, target resolution, file move/copy, and refresh.
3. Implement explorer drag-over/drop using the file-tree node-id payload, accepting only directory/root targets and performing no-overwrite `fs::rename` moves.
4. Replace context-menu clipboard placeholders with real Cut/Copy/Paste behavior and keyboard shortcuts where safe.
5. Add workspace git-status parsing/execution helpers and unit coverage for modified/added/untracked/ignored statuses.
6. Add tests for drag move and clipboard no-overwrite behavior.
7. Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
8. Mark T24 `[DONE]` in `TODO-2.md` and `TODO.md`, update the completion record, commit all relevant changes, and stop.

## Progress update
- Implemented T24 file-tree drag source payloads and Explorer drag/drop move handling.
- Implemented Explorer Cut/Copy/Paste with no-overwrite checks and recursive directory copy.
- Implemented asynchronous throttled git status collection and parser coverage.
- Validation completed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, focused tests, and `cargo test --workspace --all-targets` passed.
- Next step: update TODO records and commit T24 changes.
