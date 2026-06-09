# Execution Plan

I will follow TODO.md as the authoritative task list and complete exactly the first task whose heading is not prefixed with `[DONE]`.

## Steps
1. Read `TODO.md` first to identify the first incomplete task and its requirements.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect only the files needed for that task and avoid broad unrelated triage.
4. Implement the task as specified, without workarounds or scope narrowing.
5. Run formatting, linting, and the relevant/full test commands required by the task and repository policy.
6. If a blocking issue prevents correct implementation, add the minimum prerequisite task to `TODO.md`, commit that bookkeeping, and stop.
7. If implementation succeeds, mark the task title in `TODO.md` with `[DONE]`, update its completion record, and update this plan file with key progress.
8. Commit all task-related changes with a clear message including the required co-author trailer.
9. Stop after this one task.

## Current Status
- Plan file created.
- `TODO.md` reviewed.
- First incomplete task selected: the wrap-up item requiring full `cargo clippy` and `cargo test` to pass.
- Latest commit checked; it only records prior execution progress and does not add a directly relevant unfinished blocker.
- Working tree checked; unrelated untracked files are present and will be left untouched.
- `cargo fmt --all` completed.
- `cargo clippy --workspace --all-targets -- -D warnings` completed successfully.
- No fixture runner was found at `tools/run_fixtures.py` or another path within depth 3.
- `cargo test --all --all-targets` completed successfully.
- `TODO.md` was updated to mark the selected wrap-up validation task `[DONE]` with a completion record.
- Committed validation/task-record changes as `aef0be5` (`[Wrap-up] Validate full Rust suite`).
- Stop after this task; the next invocation should start from the next incomplete `TODO.md` item.
