# Claude Execution Plan

## Objective
Complete exactly the first incomplete task listed in `TODO.md`, using `TODO.md` as the authoritative ordering and completion source, then commit and stop.

## Current Task
- Selected first incomplete task: `R21 — 审阅 T21` from `TODO-2.md`.
- Latest commit (`[T21] Add LSP inlay hints rendering`) is directly relevant and is the implementation under review.
- Note: private chain-of-thought is intentionally not recorded here; this file contains the actionable rationale and execution plan.

## Plan
1. Inspect the T21 diff and affected files for inlay hints, composed grid rendering, theme/style mapping, config/action wiring, and tests.
2. Verify the R21 review checklist:
   - Composed grid rendering preserves existing syntax and semantic token styles.
   - Virtual text does not participate in copy/save/backing text.
   - Viewport range calculation handles soft wrap and folding without panic.
3. Fix any directly related review issues found, preferring class-wide fixes over fixture-only patches.
4. Add or adjust focused regression tests for any reviewed behavior that lacks coverage or for any bug fixed during review.
5. Run validation in order: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, then relevant/full tests.
6. Update `TODO-2.md` and `TODO.md` to mark R21 `[DONE]` with a completion record.
7. Commit all R21 changes with a descriptive message and the required co-authored trailer, then stop.

## Progress
- Plan updated for R21 before implementation changes.
- Added a focused R21 regression test covering composed inlay rendering with semantic token styling, folding markers, and copy from backing text.
- Validation passed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, focused R21 test, and `cargo test --workspace --all-targets`.
- Marked R21 as `[DONE]` in `TODO.md` and `TODO-2.md` with completion notes.
