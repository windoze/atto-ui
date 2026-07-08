# Claude Execution Plan

## Scope
- Follow `TODO.md` as the source of truth.
- Identify and complete exactly the first incomplete task whose title is not prefixed with `[DONE]`.
- Stop after marking that task done and committing the resulting changes.

## Execution Plan
1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check recent git context only as needed for the selected task, including whether the latest commit mentions unfinished work directly relevant to it.
3. Inspect the code and tests related to that task.
4. Implement the minimal correct change without workarounds or spec deviations.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the required test suite, using the full suite when code changes require it.
6. If tests fail, fix the failure or add the minimum explicit prerequisite/follow-up task to `TODO.md` according to the policy.
7. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling in its completion record.
8. Update this file when key steps complete or the plan changes.
9. Commit all relevant changes with a descriptive task-scoped commit message.

## Current Status
- First incomplete task identified: `P1.0 语法高亮方案选型`.
- Latest commit is `8452e4d Update doc`; it does not mention unfinished work directly relevant to P1.0.
- Relevant current state: `atto-ui-markdown` and `atto-ui-chat` have no syntax-highlighting dependency; `atto-ui-editor` already uses tree-sitter related crates for editor syntax features.
- Current plan is to update `AGENT_GAP.md` with the evaluated choice and interface draft, then mark P1.0 complete in `TODO.md` after validation.
- `AGENT_GAP.md` now records the P1.0 decision: use syntect with default features disabled and `regex-fancy`/`default-syntaxes`, plus a neutral highlighter interface draft for markdown code blocks and chat diff payloads.
- Validation progress: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build --workspace --all-targets` passed.
- Full `cargo test --all --all-targets` initially failed once at `tests/pty_rich_text.rs::pty_rich_text_handles_link_click` with an empty screen timeout. The exact test then passed repeatedly, and a full rerun of `cargo test --all --all-targets` passed. Kept a small diagnostic enhancement in that test so future failures include raw PTY output instead of only an empty screen.
- `TODO.md` now marks P1.0 as `[DONE]` and includes the selected syntect route, interface draft summary, validation commands, and the transient PTY-test investigation record.
- Next step: inspect final git status/diff, then commit all task-related changes with a P1.0-scoped message and stop.
