# Claude Execution Plan

## Scope
- Follow `TODO.md` as the authoritative source.
- Identify and complete exactly the first incomplete task, defined as the first task whose heading is not prefixed with `[DONE]`.
- Stop after committing that task or, if blocked, after recording the minimum prerequisite task and committing the bookkeeping change.

## Step-by-Step Plan
1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Inspect the latest commit only for directly relevant unfinished work tied to that task.
3. Read the relevant source and test files for the selected task.
4. Implement the smallest correct change that satisfies the task without workarounds or scope narrowing.
5. Add or update focused tests required by the task.
6. Run `cargo fmt`.
7. Run `cargo clippy --all-targets -- -D warnings`.
8. Run the relevant test suite, and run the full suite when required by the task or by code changes.
9. Fix any observed unscheduled failures before marking the task complete, or add the minimum prerequisite task before the blocked task in `TODO.md` and stop.
10. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling its completion record.
11. Update this plan file with progress notes for key milestones.
12. Inspect git status, diff, and recent log, then commit all intended changes with a descriptive message.
13. Stop without starting the next task.

## Progress
- Initial execution plan created before project inspection.
- Selected first incomplete task from `TODO.md`: `P5.1 会话内搜索`.
- Current scope: implement in-chat search in `crates/atto-ui-chat/src/list.rs`, including keyword highlighting, previous/next navigation, exit behavior, and off-screen hit jumps coordinated with list scrolling/virtualization.
- Implementation approach chosen: keep search state in `ChatMessageList`, intercept `Ctrl+R`/search keys before row dispatch, compute matches from chat row text, use virtual row scroll adjustments for off-screen jumps, and apply final cell-level highlighting so Markdown/diff/ANSI styling paths do not need invasive rewrites.
- Implemented initial P5.1 code and added three unit tests covering visible highlighting, next/previous off-screen jumps, and Esc scroll restoration.
- Verification so far: `cargo test -p atto-ui-chat chat_search --lib` passed.
- Full validation passed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets`, and `cargo test --all --all-targets`.
- `TODO.md` now marks `P5.1 会话内搜索` as `[DONE]` with completion notes.
- Post-diff review tightened search interaction details: search-active message updates clear stale one-shot auto-scroll suppression, and non-key events continue through normal list handling.
- Final revalidation after the follow-up patch passed with the same formatting, clippy, build, and full test commands.
