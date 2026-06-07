# Execution Plan

## Current Objective

Complete exactly the first incomplete task listed in `TODO.md`, then stop after validation, documentation updates, and a Git commit.

Selected task: `R6 — 审阅 T6` from `TODO-2.md`.

## Plan

1. Read `TODO.md` first to identify the first task whose title is not prefixed with `[DONE]`.
2. Inspect only the files and code paths needed for that task, plus recent Git context if it is directly relevant to the selected task.
3. Implement the selected task completely, or add the minimum prerequisite task to `TODO.md` if a concrete blocker makes correct implementation impossible.
4. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full tests required by the task.
5. Address any observed failing tests or fixtures according to the failure policy before marking the task complete.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling in its completion record.
7. Update this file when key steps are completed or if the plan changes.
8. Review Git status/diff/log, stage only intended files, and commit the completed task with a clear message.
9. Stop without starting the next task.

## Progress

- Initial execution plan written.
- Read `TODO.md` and `TODO-2.md`; identified `R6 — 审阅 T6` as the first incomplete task.
- Latest commit is `[T6] Wire stage three editor actions`, directly relevant to this review.
- Reviewed T6 editor action dispatch, keymap bindings, read-only gating, text/LSP sync path, and language comment configuration.
- Validation passed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
- Marked `R6` complete in `TODO.md` and `TODO-2.md` with a completion record.
