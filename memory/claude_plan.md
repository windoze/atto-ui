## 执行计划

1. 读取 `TODO.md`，定位首个标题未带 `[DONE]` 的任务。
2. 只检查当前任务需要的上下文；若最新提交明确提到与该任务直接相关的未完成问题，则纳入当前任务处理。
3. 完整实现当前任务；若遇到阻塞正确实现的具体前置问题，则在 `TODO.md` 中插入最小必要前置任务并停止。
4. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、以及所需完整测试套件。
5. 验证通过后，在 `TODO.md` / `TODO-2.md` 中把任务标题标记为 `[DONE]` 并补充完成记录。
6. 仅在阶段级计划变化时更新 `PLAN.md`；本任务不需要更新 `PLAN.md`。
7. 检查 Git 状态、差异和最近提交，只暂存本任务相关文件并提交，然后停止。

## 进展

- 已确定首个未完成任务为 `TODO-2.md` 中的 `T13 — Command registry 与 which-key popup`。
- 已检查最近提交摘要：`5419f78 [R12] Review key sequence engine`，未发现与 T13 直接相关的显式未完成问题。
- 已实现框架层 command registry、which-key model、Desktop overlay、主题 token、`atto-editor-app` command registry、未消费按键驱动的 prefix keymap，以及 editor/LSP action 的命令队列转发。
- 已通过验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。
- 已更新 `TODO.md` 与 `TODO-2.md`，将 T13 标记为 `[DONE]` 并写入完成记录。
- 下一步：暂存本任务相关文件并提交。
