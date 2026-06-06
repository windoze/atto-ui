# Execution Plan

I cannot record private chain-of-thought, but I will keep this file updated with the actionable plan, rationale, progress, blockers, and validation results.

## Current Invocation Plan

1. Read `TODO.md` to identify the first task whose title is not prefixed with `[DONE]`.
2. Review only the context needed for that task, including `PLAN.md` and relevant source/tests if required.
3. Implement the task exactly as specified, without narrowing scope or using workarounds.
4. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task and repository policy.
5. Update this file after key milestones or any plan changes.
6. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record.
7. Inspect git status/diff/log, commit all intended changes with a task-specific message, and stop without starting the next task.

## Progress Log

- Initial plan recorded before reading project files or running commands.
- `TODO.md` first incomplete task identified: `T10 — chat / terminal 测试补齐（A.2 P1）`.
- Latest commit is `[R9] Review streaming markdown tolerance`; it does not explicitly mention an unfinished issue that preempts T10.
- Working tree already contained unrelated uncommitted changes before T10 work; I will not revert them and will stage only intended T10/progress files unless the task instructions require otherwise.

## T10 Execution Scope

1. Inspect current chat and terminal PTY/integration tests plus the relevant fixtures.
2. Add coverage for chat streaming append behavior, auto-follow versus paused scroll, and text/choice/confirm input submission callbacks.
3. Add coverage for terminal mouse encoding matrix, DSR responses including split packets, bracketed paste, resize propagation, and application cursor key encoding.
4. Prefer existing fixtures and APIs; if a missing test-host capability blocks spec-correct coverage, add the minimum prerequisite task instead of working around it.
5. Validate with formatting, clippy, targeted tests, then full workspace tests.

## T10 Progress

- Confirmed current chat PTY coverage only checked mode switching, load-more, and streaming markdown tolerance.
- Confirmed current terminal PTY coverage only checked scrollback/color, capture, one SGR click, and window interactions.
- Fixed chat auto-follow semantics so message changes follow the tail only while the user remains at the bottom; user scroll-up pauses following and scrolling back to bottom resumes it.
- Extended the chat snapshot fixture with deterministic submit callback output and streaming/auto-follow append commands.
- Added chat PTY tests for text/choice/confirm submissions, auto-follow pause/resume, and streaming delta accumulation.
- Fixed terminal DSR tail handling so complete DSR requests do not remain buffered and repeat on later output.
- Fixed terminal X10/default mouse release encoding so release events use button code 3 rather than duplicating press bytes.
- Added terminal integration/unit coverage for mouse protocol/encoding/modifier matrix, split DSR packets, bracketed paste, application cursor keys, and draw-time resize sizing.
- Validation passed: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test -p atto-ui-chat`; `cargo test -p atto-ui-terminal`; `cargo test --workspace --all-targets`.
