# 执行计划

此文件记录本次调用的可公开执行计划与进度；不会记录私有推理过程。

## 范围

- 以 `TODO.md` 作为唯一任务来源。
- 只识别并完成第一个标题未带 `[DONE]` 的任务。
- 完成该任务的实现、验证、记录和提交后停止，不进入下一项。

## 步骤

1. 读取 `TODO.md`，确定第一个未完成任务和验证要求。
2. 检查最新提交是否有与该任务直接相关的未完成事项。
3. 阅读当前任务所需的实现代码和测试。
4. 如任务、阻塞点或执行路径发生实质变化，及时更新本文件。
5. 以最小正确改动完成任务要求。
6. 先运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行所需测试；完整测试使用长超时。
7. 若验证发现未计划的失败，立即修复，或在 `TODO.md` 中加入最小前置任务后停止。
8. 在 `TODO.md` 中将已完成任务标题标记为 `[DONE]`，并写入完成记录。
9. 提交本任务相关改动。
10. 停止，不处理下一项任务。

## 当前状态

- 已选择第一个未完成任务：`M5.5 Accept/Reject 流程`。
- 最新提交 `29ecadb [M5.4] Wire plan block decisions` 未声明与本任务直接相关的未完成阻塞项。
- 已确认现有代码能生成并锁定 `PlanBlock` 决策，但 Accept 后不会继续执行，Reject 后也没有显式释放计划 turn。
- 已完成 M5.5 实现：Accept 会追加内部执行指令并启动 direct 执行 turn；Reject 会释放计划 turn 且不启动执行。
- 已完成验证：`cargo fmt --all`、M5.5 针对性测试、`cargo test -p atto-agent-app --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check` 均通过。
- 已更新 `TODO.md`，将 `M5.5` 标记为 `[DONE]` 并写入完成记录。
- 下一步：提交本任务变更。
