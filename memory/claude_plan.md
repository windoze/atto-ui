# Claude Execution Plan

## Scope

- Work from `TODO.md` as the authoritative task list.
- Select the first task whose heading is not prefixed with `[DONE]`.
- Complete exactly one task in this invocation, then stop.

## Execution Plan

1. Read `TODO.md` and identify the first incomplete task before broad triage.
2. Check recent Git context only as needed to determine whether the latest commit mentions an unfinished issue directly relevant to that task.
3. Inspect the code and tests required for that task.
4. Implement the smallest spec-correct change that fully satisfies the task, without workarounds or scope narrowing.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the required test suite unless the task is documentation-only and a previous green result can be reused.
6. If tests or fixtures fail and the failure is not explicitly scheduled, fix it or add the minimum prerequisite task in `TODO.md` before marking anything complete.
7. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record.
8. Commit all intended changes with a clear task-specific commit message.
9. Stop without starting the next task.

## Progress Log

- Started invocation and recorded the execution plan before inspecting project files.
- Read `TODO.md`; selected first incomplete task: `T9 — 流式 markdown 容错增量渲染（C.1）`.
- Next step: inspect recent Git context for directly relevant unfinished work, then inspect markdown/chat rendering paths.
- Implemented the first pass for T9: markdown streaming-tolerant parsing, chat row reuse for text deltas, parser/unit coverage, and a chat PTY fixture for unclosed fences and incomplete tables.
- Next step: run formatting, linting, and tests; fix any failures before updating `TODO.md`.
- Validation passed: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p atto-ui-markdown`, `cargo test -p atto-ui-chat`, and `cargo test --workspace --all-targets`.
- Marked T9 as `[DONE]` in `TODO.md` with completion notes. Next step: inspect intended diff/status and commit only this task's changes plus the required memory/TODO updates.
