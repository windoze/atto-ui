# Execution Plan

I cannot record private chain-of-thought, but this file will track the actionable plan, key decisions, and progress for this invocation.

## Current Objective
Complete exactly the first incomplete task listed in `TODO.md`, mark only that task as `[DONE]`, commit the completed work, and stop.

Selected task: `R20 — 审阅 T20` from `TODO-2.md`.

## Step-by-Step Plan
1. Read `TODO.md` first and identify the first task whose title is not explicitly prefixed with `[DONE]`.
2. Read only the files needed to understand that task and its validation requirements.
3. Check the current git state so any existing uncommitted work is preserved and accounted for.
4. Implement the task as specified, without narrowing scope or using workaround behavior.
5. Run formatting, linting, and relevant tests in the required order; address any unscheduled failures according to the task policy.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and adding a completion record with validation results.
7. Update this file at key milestones or if the plan changes.
8. Commit all files relevant to this completed invocation with a clear message and the required co-author trailer.
9. Stop after the first incomplete task is completed.

## Progress
- Initial plan recorded.
- Read `TODO.md`; first incomplete task is `R20`.
- Read the `R20` source entry in `TODO-2.md`.
- Latest commit is `[T20] Add LSP formatting support`, directly relevant to `R20`.
- Review found a format-on-save failure gap: if the LSP transport/session fails while formatting is pending, no failed `FormatFinished` event was emitted.
- The first disconnect fixture exposed a related clean-EOF/no-response gap; implemented pending-format timeouts so format-on-save cannot remain silently pending.
- Implemented pending formatting failure completion on LSP poll errors and no-response timeouts, plus a mock-server clean-exit regression test.
- Validation passed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted LSP regression, and `cargo test --workspace --all-targets`.
- Marked `R20` as `[DONE]` in `TODO.md` and `TODO-2.md` with completion notes.
