# Claude Execution Plan

## Objective
Complete exactly the first incomplete task listed in TODO.md, then stop after marking it done and committing the result.

## Plan
1. Read TODO.md to identify the first task whose heading is not prefixed with [DONE].
2. Review the selected task requirements, dependencies, validation instructions, and completion-record format.
3. Inspect only the relevant project files for that task and check the latest commit for any directly relevant unfinished note.
4. Implement the task without workarounds or scope narrowing; if a concrete blocker appears, add the minimum prerequisite task to TODO.md instead.
5. Run formatting, linting, and the relevant tests required by TODO.md; address any unscheduled failures before marking the task done.
6. Update TODO.md by prefixing the completed task title with [DONE] and filling its completion record. Update PLAN.md only if phase-level sequencing changes.
7. Commit all task-related changes with a clear message and the required co-author trailer.

## Progress
- Initial plan recorded before task execution.
- Selected first incomplete task: `T17 — Workspace / LSP Bridge 状态层` from `TODO-2.md`.
- Added shared workspace-state and workspace-LSP bridge modules.
- Wired editor-window tab open/save/close/active-tab paths to the shared workspace.
- Routed workspace-symbol requests through the workspace LSP bridge and added workspace LSP polling to the app tick loop.
- Validation completed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Marked T17 `[DONE]` in `TODO.md` and `TODO-2.md`.

## T17 Execution Steps
1. Inspect the existing editor app tab/window/open/save flow and current LSP integration points.
2. Inspect available `editor-core` workspace and `editor-core-lsp` workspace sync APIs to use their URI helpers and avoid duplicating protocol logic.
3. Add workspace state and LSP workspace bridge modules for buffer identity, path-to-buffer reuse, tab binding synchronization, workspace edit application, and active-document tracking.
4. Wire the bridge into file open, tab switching, save/edit synchronization, tick polling, and any existing workspace-symbol path that needs shared LSP state.
5. Add focused tests for duplicate open buffer reuse, multi-tab workspace edit propagation, active tab LSP tracking, and preservation of dirty/open behavior.
6. Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
7. Mark T17 `[DONE]` in `TODO-2.md` and update `TODO.md`, then commit all task-related files.
