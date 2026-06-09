# Claude Execution Plan

## Current objective
- Complete the first incomplete task in `TODO.md`: T27 — Jumplist / registers 设计占位与 WorkspaceEditorView 决策.
- Use `TODO-2.md` as the task detail source and stop after T27 is complete.
- Avoid adding unused prototype code unless the task requires it.

## T27 execution plan
- Read T27 requirements and the existing workspace bridge design.
- Compare the bridge route (`EditorView + Binding<String>` with app-level `WorkspaceState`) against a future `WorkspaceEditorView`.
- Record a concrete decision in `PLAN-2.md`, naming the future type/file and the trigger for switching.
- Add a code-level comment at the current bridge boundary in `workspace_state.rs`.
- Run formatting. Skip full clippy/test because the changes are documentation/comment-only and the previous full validation is already recorded green.
- Mark T27 `[DONE]` in `TODO.md` and `TODO-2.md`, update the completion record, commit, and stop.

## T27 progress
- First incomplete task identified: T27 in `TODO-2.md`.
- Latest commit is `[R26] Review trim trailing whitespace save flow`; it does not mention an unfinished issue relevant to T27.
- Decision implemented as documentation/comment-only: keep production in bridge mode, require a workspace-backed `WorkspaceEditorView` before jumplist/registers, and avoid unused prototype code.
- `cargo fmt` passed.
- T27 marked `[DONE]` in `TODO.md` and `TODO-2.md`; completion record documents the decision and validation scope.

## Previous invocation notes
- Plan file initialized.

## Selected task
- First incomplete task for this invocation: R26 — 审阅 T26 (TODO-2.md · 阶段五).
- Latest commit is `[T26] Add trim trailing whitespace on save`, directly relevant to R26.
- Review scope: confirm trim does not delete line-internal spaces, final newline state is preserved, and failed saves do not clear dirty markers.

## T26 requirements
- Implement trim-trailing-whitespace in editor save flow.
- Ensure save/format-on-save/save-as ordering is explicit and consistent.
- Add/update tests for trim behavior, dirty state, and save-after-format behavior.
- Preserve spec-correct behavior; do not silently work around missing editor/workspace features.

## Implementation plan update
- Add a shared trim-trailing-whitespace binding beside format_on_save and pass it into primary/secondary EditorView configs.
- Generate pre-edit character-offset TextEditSpec deletions for ASCII spaces/tabs at line ends, preserving line terminators and final-newline state.
- Apply trim edits to workspace buffers before writing so the operation is undoable and tab bindings update; fallback to binding text for no-workspace tabs.
- Reuse save_tab_at for save-after-format so the order remains format -> trim -> write -> clean marker.

## Implementation progress
- Added trim_trailing_whitespace_on_save to EditorConfig and dynamic component properties.
- Wired the binding through editor tabs and split editor views.
- Save and Save As now run save transforms before writing; format-on-save still completes before calling the shared save path.
- Trim edits are generated as character-offset deletions and applied through workspace apply_text_edits when a workspace buffer exists.
- Added unit coverage for default no-trim saves, enabled trim saves, dirty clearing, line-internal spaces, CRLF preservation, and final-newline preservation.

## Validation progress
- cargo fmt passed.
- cargo clippy --workspace --all-targets -- -D warnings passed.
- Running cargo test --workspace --all-targets next.

## Validation result
- cargo test --workspace --all-targets passed.
- No code changes have been made after validation yet; upcoming TODO/memory updates are documentation/progress records only.

## Completion record
- T26 marked [DONE] in TODO.md and TODO-2.md with implementation and validation notes.
- PLAN.md unchanged because phase-level sequencing did not change.

## Revalidation update
- Added a focused unit test for the save-after-format event path trimming before write.
- Re-running cargo fmt, clippy, and cargo test because code changed after the prior validation.
- Revalidation passed: cargo fmt; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace --all-targets.

## R26 review progress
- Reviewed the T26 implementation and found two blocking gaps: Save As skipped format-on-save, and failed write dirty-marker behavior lacked regression coverage.
- Replaced the boolean save-after-format state with a pending save action that carries either normal Save or a Save As target through formatter completion.
- Routed Save As through the same format-on-save completion path as Save, preserving the selected target path.
- Added regression coverage for Save As format-before-trim/write ordering and dirty marker preservation when the final write fails.
- Validation passed after the fixes: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace --all-targets`.
- No `tools/run_fixtures.py` fixture runner exists in this repository.
- R26 marked `[DONE]` in `TODO.md` and `TODO-2.md`; `PLAN.md` unchanged because phase-level sequencing did not change.
