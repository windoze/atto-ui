# Claude Execution Plan

## Current objective
- Create this progress file before implementation work.
- Read TODO.md and identify the first task whose title is not prefixed with [DONE].
- Review only the files and context needed for that task.
- Implement the task exactly as specified, or add the minimum prerequisite task if a concrete blocker prevents correct implementation.
- Run formatting, linting, and relevant tests as required by TODO.md and repository policy.
- Update TODO.md completion status and record validation results.
- Commit all task-related changes and stop without starting the next task.

## Progress
- Plan file initialized.

## Selected task
- First incomplete task: T26 — Trim trailing whitespace 与 save 流程整理 (TODO-2.md · 阶段五).
- Latest commit is an R25 note and does not introduce an unfinished issue directly relevant to T26.
- Next step: read T26 details and inspect the editor save/formatting code paths.

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
