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

## R10 执行计划

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 识别第一个未完成任务。
2. 查看最近提交信息，确认是否有与该任务直接相关的未完成问题需要纳入当前任务或作为前置任务记录。
3. 查看 `ff84431 [T10] Add chat and terminal coverage` 的变更范围，聚焦 chat PTY、terminal 集成测试、相关 fixture 与行为修复。
4. 审阅 terminal 鼠标编码矩阵，确认 Down/Up/Drag/Move/Scroll、SGR/X10、modifier、协议模式组合是否被系统覆盖。
5. 审阅 chat 自动跟随、用户上滚暂停、回到底部恢复以及 text/choice/confirm input 提交边界。
6. 若发现真实缺陷，实施最小正确修复并补充回归测试；若未发现缺陷，仅记录审阅结果。
7. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关测试以及必要的全 workspace 测试。
8. 将 R10 标记为 `[DONE]` 并补充完成记录，然后提交本次审阅结果。

## R10 当前进度

- 已读取 `TODO.md`，第一个未完成任务是 `R10 — 审阅 T10`。
- 最近提交 `ff84431 [T10] Add chat and terminal coverage` 与当前审阅任务直接相关，已作为主要审阅对象。
- 已审阅 terminal 鼠标矩阵覆盖，发现原测试缺少中键/右键、部分 modifier 组合和 ScrollUp/Left/Right；已扩展 `input_encoding.rs` 覆盖完整组合。
- `cargo clippy --workspace --all-targets -- -D warnings` 首次运行发现扩展后的测试匹配分支已穷尽，兜底分支不可达；已移除不可达分支并复跑通过。
- 已完成 R10 审阅并更新 `TODO.md`：chat 自动跟随/暂停边界通过审阅，terminal 鼠标矩阵覆盖缺口已补齐。
- 验证通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal`、`cargo test -p atto-ui-chat`、`cargo test --workspace --all-targets`。
- 已确认工作区存在与本次 R10 无关的既有改动，提交时只纳入 `TODO.md`、`crates/atto-ui-terminal/tests/input_encoding.rs` 和本文件。
