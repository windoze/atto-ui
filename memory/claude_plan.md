# 当前执行计划

## 约束
- 以 `TODO.md` 为任务顺序与完成状态的唯一依据。
- 本轮只完成第一个标题未带 `[DONE]` 的任务，然后停止。
- 不做开放式历史问题扫描；只处理当前任务相关或测试暴露且未排期的问题。
- 若发现阻塞当前任务的具体前置问题，更新 `TODO.md` 插入最小前置任务，提交后停止。
- 完成任务后必须更新 `TODO.md` 的标题 `[DONE]` 与完成记录，必要时更新 `PLAN.md`。
- 提交前检查 `git status`、`git diff`、近期日志，并只提交本轮相关变更。

## 步骤
1. 读取 `TODO.md`，按文件顺序确定第一个标题未带 `[DONE]` 的任务。
2. 检查最近提交信息，只有在其明确提到与当前任务直接相关的未完成事项时，才纳入当前任务或作为前置任务记录。
3. 阅读当前任务涉及的代码、测试和文档，确认要求、依赖和验证方式。
4. 如任务可直接完成，实施最小正确代码或文档变更；如存在必须先修复的阻塞问题，更新 `TODO.md` 记录前置任务并停止。
5. 按要求运行格式化、lint 和相关测试；若代码有变化，顺序为 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试。
6. 若测试失败且未在后续任务明确排期，优先修复；无法在当前任务内合理修复时，将最小修复任务插入 `TODO.md` 的正确位置。
7. 完成后更新 `TODO.md`：给当前任务标题加 `[DONE]`，填写完成记录、验证结果和说明。
8. 仅当阶段计划、依赖或完成标准发生变化时更新 `PLAN.md`。
9. 更新本文件记录关键进展和最终验证结果。
10. 检查 `git status`、`git diff`、`git log --oneline -10`，提交本轮所有相关变更，提交信息包含任务编号与动作。
11. 停止，不继续处理下一个任务。

## 当前状态
- 已识别本轮任务：`M6.6 Transcript 持久化（可选）`。
- 最近提交为 M6.5 完成记录和实现提交，未发现直接要求并入 M6.6 的未完成事项。
- 已实现默认关闭的 transcript 持久化配置：TOML `transcript_path`、环境变量 `ATTO_AGENT_TRANSCRIPT`、CLI `--transcript`。
- 已新增 app crate 私有 JSONL transcript 格式，覆盖当前 `ChatMessage` / `ChatBlock` 已知变体、meta、审批、错误和嵌套 task transcript；恢复时将未完成 streaming turn 标为 canceled。
- 已在真实运行路径接入启动恢复、dirty observer 自动保存和退出最终保存；snapshot 默认配置不启用持久化。
- 已运行并通过：`cargo fmt --all`；`cargo test -p atto-agent-app transcript`；`cargo test -p atto-agent-app config::tests`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`，将 M6.6 标记为 `[DONE]` 并写入完成记录。
- 下一步检查 git 状态和 diff，确认仅提交本轮相关变更，然后提交并停止。
# Execution Plan

## Current Invocation

This file records the actionable plan and progress for the current TODO-driven invocation. It intentionally captures decisions, validation steps, and status updates rather than private chain-of-thought.

## Plan

1. Read `TODO.md` first and identify the first task whose title is not prefixed with `[DONE]`.
2. Check the latest commit only for an explicitly mentioned unfinished issue that is directly relevant to that task.
3. Read the selected task details, including requirements, dependencies, and validation requirements.
4. Inspect only the code and tests needed to implement that task; avoid broad unrelated triage.
5. Implement the task completely unless a concrete prerequisite blocker is discovered.
6. If blocked, add the minimum prerequisite task to `TODO.md`, keep the current task incomplete, commit the task-list update, and stop.
7. If implemented, run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required.
8. Fix any observed failing tests unless they are already explicitly scheduled in `TODO.md`.
9. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record.
10. Commit all changes for exactly this task with a clear message, then stop without starting the next task.

## Progress

- Started invocation and recorded the initial execution plan before running repository commands.
- Read `TODO.md`; the first incomplete task is `M6.7 状态栏完善`.
- Scope for this invocation: improve the agent app status bar so it shows model, plan mode, tool count, loaded skill count, streaming state, token estimate, and latest error summary; then validate, update `TODO.md`, commit, and stop.
- Implemented the status bar changes in `crates/atto-agent-app`: added tool count, transcript token estimate, and latest error summary state; wired transcript status refresh into user submit, slash commands, cancellation, plan decisions, tool approval, and app actions.
- Added focused unit coverage for required status bar segments, initial state properties, and transcript token/error summaries.
- During full test validation, `tool_call_budget_fails_turn_before_appending_over_limit` exposed an early return in the tool-budget failure action path that skipped status synchronization. Reworked that branch to return through the common action path; the targeted test now passes.
- Validation completed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p atto-agent-app --lib tool_call_budget_fails_turn_before_appending_over_limit`, `cargo test --workspace --all-targets`, and `cargo fmt --all -- --check` all passed.
