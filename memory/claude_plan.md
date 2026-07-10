# 当前执行计划

## 范围

- 以 `TODO.md` 作为权威任务列表和完成状态来源。
- 只处理标题未带 `[DONE]` 的第一个任务。
- 本文件记录可审查的执行计划和进度，不记录私有推理细节。

## 步骤

1. 读取 `TODO.md`，确认第一个未完成任务。
2. 仅检查最新提交中与当前任务直接相关的未完成事项。
3. 阅读当前任务涉及的代码、配置和文档。
4. 用小范围补丁完整实现当前任务。
5. 按顺序运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`，再运行 `cargo fmt --all -- --check`。
6. 若发现未安排的测试失败，先修复或在 `TODO.md` 中加入最小必要前置任务。
7. 将当前任务标题标记为 `[DONE]`，并填写完成记录和验证记录。
8. 检查 `git status`、`git diff` 和最近提交，提交本任务变更。
9. 若 `TODO.md` 已全部完成，完成最终复核并创建 `endtag`。

## 进度

- 已在执行仓库命令前记录初始计划。
- 已选定第一个未完成任务：`TODO.md` 中的 `Release 检查`。
- 已确认目标：记录新增 `atto-agent-app` crate 的发布策略并提交该任务。
- 已完成发布范围变更：`atto-agent-app` 设置为 `publish = false`，`docs/RELEASE.md` 说明其为 workspace-only app，根 README 指向该 release scope。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已更新 `TODO.md`：`Release 检查` 标题已标记 `[DONE]`，并补充完成记录和验证记录。
- 已完成最终复核：`TODO.md` 最后一项收尾任务已完成，`PLAN.md` 阶段范围均由已完成 TODO 记录覆盖，完整 fmt、clippy 和 workspace 测试均通过。
- 下一步：提交最终任务变更并创建 `endtag` 标记。
