# 当前执行计划

说明：本文件记录本次调用的可审阅执行计划、关键进展和验证结果；不包含私有推理过程。

## 计划

1. 读取 `TODO.md`，按文件顺序识别第一个标题未带 `[DONE]` 的任务。
2. 仅检查与所选任务直接相关的最近提交信息，不做开放式历史问题扫描。
3. 阅读任务要求与相关源码，确认实现边界和验证要求。
4. 完整实现当前任务，不通过缩窄范围或 workaround 规避需求。
5. 先运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过后运行完整测试套件。
6. 如验证发现未排期失败，立即修复或在 `TODO.md` 中插入最小必要前置任务。
7. 将完成任务标题标记为 `[DONE]`，并更新完成记录。
8. 检查 git 状态、diff 和最近提交，提交本次任务相关全部变更，然后停止。

## 进展

- 已在选择任务前记录初始计划。
- 已读取 `TODO.md`，本次第一个未完成任务为 `M3.5 Approval UI`。
- 本次范围：渲染 `ToolUseBlock.approval`，处理 allow once / allow project / deny，deny 时写入失败 tool result。
- 已确认最近提交 `[M3.4] Add mutating tools` 与本任务直接相关，但未声明未完成阻塞项。
- 已实现 agent app 审批接线：运行时权限状态、tool call 入库前权限处理、`on_approve` 回调、项目级授权和拒绝结果写入。
- 已补充单元测试覆盖审批选项生成、allow once、allow project 复用和 deny 失败结果。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`，将 `M3.5 Approval UI` 标记为 `[DONE]` 并写入完成记录。
