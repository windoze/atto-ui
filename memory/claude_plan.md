本文件记录本次执行的可公开计划与进度。不会记录隐藏推理过程。

## 执行计划

1. 读取 `TODO.md`，按文件顺序定位第一个标题未带 `[DONE]` 的任务。
2. 查看该任务的具体要求、依赖、验证标准，以及最近提交是否明确提到与该任务直接相关的未完成问题。
3. 检查当前工作区状态，避免覆盖或回退他人/用户已有改动。
4. 按任务要求做最小且完整的实现；如果发现当前任务被具体前置问题阻塞，则更新 `TODO.md` 插入最小必要前置任务并停止。
5. 运行要求的验证流程：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后在需要时运行完整测试套件。
6. 验证通过后，在 `TODO.md` 中给当前任务标题添加 `[DONE]` 并补全完成记录；仅在阶段计划变化时更新 `PLAN.md`。
7. 提交本次任务相关全部变更，提交信息包含任务编号与明确说明，然后停止，不继续下一个任务。

## 当前进度

- 已读取 `TODO.md`，第一个未完成任务是 `M3.2 Tool call 聚合`。
- 本轮只处理 `M3.2`：按 SSE `tool_calls[].index` 聚合 name/arguments，并在 `finish_reason = tool_calls` 时进入工具执行阶段。
- 下一步检查最近提交、工作区状态，以及读取与 DeepSeek SSE 映射、chat block/tool 抽象相关的实现。
- 已检查最近提交和工作区：最新提交为 `[M3.1] Add tool abstractions`，未声明直接相关未完成事项；当前未提交变更只有本次计划文件。
- 已定位实现边界：`deepseek.rs` 已反序列化 `ChatToolCallDelta`，缺口在 `stream_ui.rs` 尚未聚合 tool call delta，也没有向主线程发出工具请求动作。
- 当前实现方案：在 stream mapper 中维护按 `(choice.index, tool_calls[].index)` 排序的聚合状态，拼接 id/name/arguments；当 `finish_reason = tool_calls` 时解析 arguments JSON，生成 `ToolUseBlock` 插入 assistant turn，并把 turn 标记为完成且 meta stop reason 为 tool use。后续 M3.3-M3.6 再接真实执行、审批和回灌。
- 已完成实现初稿：新增 tool call 聚合器、`ToolCallsReady` action、主线程插入 `ToolUseBlock`，并补充多 tool call 聚合与非法 arguments 的单测。
- 已运行验证并通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已更新 `TODO.md`：`M3.2 Tool call 聚合` 已标记为 `[DONE]`，完成记录包含实现摘要与验证命令。
- 已完成提交前检查：`git status --short`、`git diff`、`git log --oneline -10`，变更范围确认属于本轮 `M3.2`。
- 下一步提交本次变更并停止。
