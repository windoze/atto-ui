# 执行计划

## 当前目标

- 按 `TODO.md` 的顺序完成第一个未标记 `[DONE]` 的任务，然后停止。
- `TODO.md` 是任务状态与完成记录的权威来源；仅在阶段级计划变化时更新 `PLAN.md`。

## 执行步骤

1. 读取 `TODO.md`，识别第一个标题未以 `[DONE]` 开头的任务，并记录任务要求、依赖与验证条件。
2. 检查最近提交与当前工作区状态，确认是否存在与该任务直接相关的未完成内容或需要纳入本次提交的未提交变更。
3. 根据任务要求阅读相关代码与测试，限定范围在当前任务及其直接依赖内，避免开放式历史问题扫描。
4. 如任务可直接完成，则实现最小正确改动；如发现阻塞当前任务的具体前置问题，则将最小前置任务插入 `TODO.md`，提交后停止。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试套件；若只有文档类变更且可复用上一次绿色结果，则在完成记录中说明跳过原因。
6. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，填写本次完成记录、验证结果与提交信息占位。
7. 提交所有与本任务相关的变更，提交信息使用任务编号与简短说明。
8. 停止，不继续处理下一项任务。

## 进度记录

- 已创建本计划文件，下一步读取 `TODO.md` 确定当前任务。
- 已读取 `TODO.md`，第一个未完成任务是 `M3.6 Tool result 回灌`：需要将 `ToolResultBlock` 写入 UI，并把 tool result 转换为下一次 DeepSeek request 的 `role=tool` 消息。
- 下一步检查工作区状态与最近提交，然后定位工具调用执行、UI block、DeepSeek request 构建与 transcript 上下文转换代码。
- 已确认当前工作区除本计划文件外无其它未提交变更；最近提交为 `M3.5` 审批 UI，无需插入额外前置任务。
- 已定位现状：`ChatMessageStore::upsert_tool_result` 已支持 UI 写入，但 app 尚未执行 `Running` 工具；`deepseek.rs` 已有 `ChatCompletionMessage::tool(...)`，但 app 尚未把 UI transcript 转换为下一次 request。
- 修订执行步骤：新增后台工具执行 action、执行获准工具并回写 `ToolResultBlock`；新增最小 transcript -> DeepSeek messages/request 转换函数；补单元测试验证 tool result UI 和 `role=tool` request 映射。
- 已实现上述代码路径与单元测试，准备运行 `cargo fmt --all`。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 下一步更新 `TODO.md`，将 `M3.6` 标记为 `[DONE]` 并填写完成记录。
- 已更新 `TODO.md`，`M3.6 Tool result 回灌` 已标记为 `[DONE]` 并写入完成记录。
- 提交前检查显示变更仅包含 `TODO.md`、`crates/atto-agent-app/src/lib.rs`、`memory/claude_plan.md`；下一步提交本任务。
- 已提交任务实现，提交为 `61b1e32 [M3.6] Add tool result feedback`。
