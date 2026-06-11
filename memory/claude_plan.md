# Claude Execution Plan

## Current Objective

Complete exactly the first incomplete task in `TODO.md`: `收尾 2 — 快照人工比对`, then commit the completed work and stop.

## Execution Plan

1. Read `TODO.md` and identify the first task whose heading is not prefixed with `[DONE]`.
2. Read the relevant task details, dependencies, validation requirements, and any completion record for that task.
3. Check recent repository state only as needed for the selected task, including the latest commit if it directly mentions unfinished work relevant to that task.
4. Inspect the relevant source and test files for the selected task.
5. Implement the task without changing scope or using workarounds.
6. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the required tests with adequate timeout.
7. If validation exposes an unscheduled failure, fix it if in scope or add the minimum prerequisite task to `TODO.md`, commit that scheduling change, and stop.
8. When the task is complete, mark its title in `TODO.md` with `[DONE]` and update its completion record.
9. Commit all task-related changes with a clear task-specific commit message.
10. Stop without starting the next task.

## Progress Log

- Initialized execution plan before inspecting project task details.
- Identified the first incomplete task as `收尾 2 — 快照人工比对`.
- Next focus: inspect `CHAT_UI.md` capability matrix, snapshot app scenarios, and task-specific validation requirements before making any code or documentation changes.
- Reviewed `CHAT_UI.md` §2, `PLAN.md`, and `snapshot_chat_app`; the remaining work is a documentation/audit task because P8.1-P8.3 have already added the missing scenarios and tests.
- Latest commit is `[P8.3] Add chat text selection`, which is directly relevant but does not record unfinished follow-up work.
- Current validation approach: capture representative `snapshot_chat_app` PTY screens for the capability matrix, update `CHAT_UI.md` §2 to reflect all rows as implemented, then update `TODO.md` with the audit record.
- First temporary audit run failed because the helper scrolled past the block-mapping viewport and triggered load-more before checking ANSI output; adjust the helper's scroll windows and rerun.
- Temporary PTY audit test passed after adjustment and printed 18 snapshot sections: streaming markdown, block mapping top/bottom, long tool output tail/expanded, inline approval, inline diff, plan, nested task, thinking/notice, todo, turn meta/error, message actions, text selection, cancel, responsive narrow/wide, and artifact links.
- Removed the temporary audit test so final code changes remain limited to documentation/progress files unless a real gap is found.
- Updated `CHAT_UI.md` §2 to the final capability coverage matrix with every previously missing/partial row marked ✅ and linked to implementation/snapshot validation points.
- Validation run so far: `cargo fmt --all -- --check` passed; `cargo test -p atto-ui-chat --test pty_chat` passed with 23 tests.
- Because the final task changes are documentation/progress files only, full build/clippy/full test can reuse the previous green P8.3/收尾 1 results unless a code change is introduced before commit.
- Updated `TODO.md` to mark `收尾 2 — 快照人工比对` as `[DONE]` with completion and validation records.
- Confirmed `TODO.md` no longer contains unchecked task headings; after committing, create `endtag` as the completion marker if the tag does not already exist.
