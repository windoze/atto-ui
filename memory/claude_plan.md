# Execution Plan

## Current understanding
- `TODO.md` is the authoritative task list and completion source.
- I must complete exactly the first task whose heading/title is not prefixed with `[DONE]`, then stop.
- I will not perform broad issue triage before selecting that task.
- I will not expose private chain-of-thought; this file records the actionable reasoning summary, decisions, and execution steps.

## Step-by-step plan
1. Read `TODO.md` and identify the first incomplete task by heading/title prefix.
2. Check recent git state only as needed to understand whether there is unfinished work directly relevant to that selected task.
3. Read the code, tests, and documentation needed for that task only.
4. Implement the task completely, avoiding workarounds or scope narrowing.
5. Update or add focused tests for the implemented behavior.
6. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite required by the task.
7. If a blocking prerequisite or unscheduled failing test is discovered, update `TODO.md` with the minimum prerequisite/follow-up task, commit that bookkeeping, and stop.
8. If implementation validates successfully, mark the task title in `TODO.md` with `[DONE]`, update its completion record, commit all task-related changes, and stop.

## Progress log
- Created this plan before inspecting project task details.
- Identified first incomplete task as `TODO-2.md` R24, the review of T24 explorer drag/cut/copy/git-status work.
- Focused review found two R24-relevant issues to fix before marking R24 complete:
  1. multi-item cut/move could partially move earlier sources if a later rename failed, leaving stale clipboard state;
  2. text porcelain git rename parsing was ambiguous for renamed paths containing ` -> `.
- Implemented rollback-aware multi-item move handling with explicit partial-failure clipboard clearing, and switched git status collection/parsing to NUL-delimited porcelain v1 output with rename-path coverage.
- Validation completed successfully with `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`; no `tools/run_fixtures.py` fixture runner exists.
- Marked R24 `[DONE]` in `TODO.md` and `TODO-2.md` with a completion record.
