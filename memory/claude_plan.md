# 执行计划

## 当前指令

以 `TODO.md` 为唯一任务来源，完成第一个未完成任务，验证通过后将任务标题标记为 `[DONE]`，提交 Git commit，然后停止。

## 决策摘要

- `TODO.md` 决定任务顺序和完成状态。
- 任务只有在标题显式带有 `[DONE]` 时才算完成。
- 已观察到的测试失败不能忽略，除非已有明确后续任务排期。
- 除非出现具体前置阻塞，否则应按当前任务原样完成，不做规避实现。
- `PLAN.md` 只在阶段级顺序、依赖或完成标准变化时更新。

## 步骤计划

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务。
2. 只检查最新提交是否提到与当前任务直接相关的未完成事项。
3. 阅读当前任务相关的 app、fixture 和 PTY 测试结构。
4. 以最小完整改动实现任务要求，不引入 workaround。
5. 添加或更新覆盖任务行为的聚焦测试。
6. 运行 `cargo fmt`。
7. 运行 `cargo clippy --workspace --all-targets -- -D warnings`。
8. 运行相关定向测试和完整 workspace 测试。
9. 若出现未排期失败，立即修复或在 `TODO.md` 中加入最小前置任务。
10. 关键步骤完成或计划变化时更新本文件。
11. 更新 `TODO.md`，给完成任务标题加 `[DONE]` 并填写完成记录。
12. 提交前检查 `git status`、`git diff` 和最近提交。
13. 用描述性提交信息提交本任务相关变更。
14. 提交后停止，不开始下一个任务。

## 进度

- 已记录初始执行计划。
- 已确认第一个未完成任务为 `M1.6 快照与 PTY`。
- 最新提交为 `[M1.5] Implement cancellation semantics`；没有显式未完成项，但 M1.6 会通过 PTY 覆盖验证 M1.5 取消行为。
- 已实现专用 `snapshot_agent_app` fixture、用于稳定取消测试的可配置 mock token delay，以及覆盖输入流式输出、slash 命令和 Esc 取消的 PTY 测试。
- 验证已通过：`cargo test -p atto-agent-app --test pty_agent`、`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、完整 `cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已更新 `TODO.md`，将 `M1.6` 标记为 `[DONE]` 并写入完成和验证记录。
