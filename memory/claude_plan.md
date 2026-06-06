# Claude Execution Plan

## Scope

- Work from `TODO.md` as the authoritative task list.
- First incomplete task selected: `R9 — 审阅 T9`.
- Complete exactly this review task in this invocation, then stop.
- This file records the execution plan and progress; it does not include hidden reasoning traces.

## Step-By-Step Plan

1. Check recent Git context for any latest-commit unfinished issue directly relevant to `R9`.
2. Inspect the T9 implementation areas listed in `TODO.md`: markdown tolerant streaming, chat message rendering/reuse, and PTY coverage.
3. Review correctness against R9 acceptance points: no completed markdown is swallowed, closed syntax recovers from fallback rendering, and incremental parsing does not reintroduce full structural rebuilds for text deltas.
4. Run validation in the required order: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, targeted chat/markdown PTY tests as needed, then the full workspace test suite unless unchanged green results can be reused.
5. If review finds a defect or an unscheduled failing test, fix it within R9 when it is a direct T9 regression; otherwise add the minimum prerequisite/follow-up task in `TODO.md` before marking R9 complete.
6. Mark `R9` as `[DONE]` in `TODO.md` and add a completion record with review findings and validation commands.
7. Commit all intended changes, including `TODO.md` and this progress file, with a clear `R9` commit message.
8. Stop without starting `T10`.

## Progress Log

- Read `TODO.md`; selected first incomplete task: `R9 — 审阅 T9`.
- Reviewed `PLAN.md`; no phase-level change is expected for a routine review task unless a blocker changes dependencies.
- Updated this execution plan before running build, test, or Git commands.
- Checked recent Git context: latest commit is `[T9] Add streaming markdown tolerance`; it does not mention an unfinished issue that changes R9 scope.
- Observed pre-existing working tree changes outside this review (`PLAN.md`, archived docs/scripts); I will not revert or include unrelated changes unless they become directly relevant.
- Review found a T9 regression: trailing incomplete-table normalization can escape pipe characters inside an unclosed fenced code block, corrupting code text. I will fix this in R9 and add a focused markdown unit test.
- Implemented the R9 fix: streaming table fallback now skips normalization while a fenced code block is unclosed, with a unit test proving table-like code text keeps literal pipe characters.
- Validation progress: `cargo fmt` and `cargo clippy --workspace --all-targets -- -D warnings` passed. `cargo test -p atto-ui-chat` passed. `cargo test -p atto-ui-markdown` exposed a PTY timeout in `pty_markdown_viewer_scrolls_code_blocks_and_tables`; I will reproduce and fix or schedule it before completing R9.
- The markdown PTY timeout reproduced as resource-sensitive startup/render waiting: single-test rerun passed. I increased that test's visible-text wait budget to 5 seconds without weakening assertions, so standard validation is less sensitive to concurrent cargo load.
- Final validation passed after fixes: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test -p atto-ui-markdown`; `cargo test -p atto-ui-chat`; `cargo test --workspace --all-targets`.
- Next step: mark `R9` as `[DONE]` in `TODO.md` with review/fix notes, then inspect the intended diff and commit only R9-related files plus this progress file.
- Marked `R9` as `[DONE]` in `TODO.md` and recorded review conclusions, the fenced-code/table fallback fix, the PTY timeout stabilization, and validation commands.
