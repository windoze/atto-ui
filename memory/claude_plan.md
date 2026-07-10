# 执行计划

本文件记录本次调用的可公开执行计划与进度。不会记录隐藏推理链，只记录可审计的计划、决策和结果。

## 当前任务

- 已读取 `TODO.md`。
- 第一个未完成任务：`M3.1 Tool 抽象`。
- 任务要求：定义 `ToolSpec`、`ToolExecutor`、`ToolRegistry`、权限策略、OpenAI tools schema 转换。

## 执行计划

1. 检查最近提交和当前 git 状态，只把与 `M3.1` 直接相关或阻塞 `M3.1` 的事项纳入范围。
2. 阅读 `PLAN.md`、`TUI_AGENT.md` 和 `crates/atto-agent-app` 现有模块，确认工具抽象应放置的位置、公开 API 边界和现有 DeepSeek tool schema 类型。
3. 设计并实现最小完整的工具抽象：工具规格、参数 schema、权限策略、执行 trait、执行上下文、工具注册表，以及到 OpenAI-compatible tools schema 的转换。
4. 为注册表、权限策略、schema 转换和重复/未知工具等行为补充单元测试。
5. 运行 `cargo fmt --all`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过后运行 `cargo test --workspace --all-targets`，最后运行 `cargo fmt --all -- --check`。
6. 若发现未安排的失败测试，优先修复；如果存在阻塞当前任务的具体前置问题，则更新 `TODO.md` 插入最小前置任务并停止。
7. 验证通过后，在 `TODO.md` 将 `M3.1 Tool 抽象` 标记为 `[DONE]` 并补充完成记录。
8. 提交本次任务相关变更，然后停止，不继续 `M3.2`。

## 进度

- 已读取 `TODO.md` 并确认当前任务。
- 已更新本计划文件。
- 最近提交为 `[M2.R] Complete DeepSeek streaming review`，未声明与 `M3.1` 直接相关的未完成事项。
- 当前工作区除本计划文件外无其它未提交变更。
- 已阅读 `PLAN.md`、`TUI_AGENT.md`、DeepSeek request/schema 代码、app crate 模块边界和 chat tool UI 数据结构。
- 确认本次只实现 `M3.1` 抽象层：新增 `tool` 模块，复用 `deepseek::ChatTool` 作为 OpenAI-compatible schema 输出；不实现内置工具、tool call 聚合、tool loop 或审批 UI。
- 已新增 `crates/atto-agent-app/src/tool.rs`，实现 `ToolSpec`、`ToolExecutor`、`ToolRegistry`、`ToolPermissionPolicy`、`ToolPermissionDecision`、`ToolContext`、`ToolResult` 和 OpenAI-compatible tools schema 转换。
- 已补充工具 schema、注册表、重复/未知工具、权限策略和 spec 校验单元测试。
- `cargo fmt --all`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test --workspace --all-targets` 首次运行发现 `agent_slash_commands_render_outputs_and_update_state` 仍等待旧 `/tools` 文案。
- 已更新 PTY 测试期望为当前 `/tools` 输出中的 `Tools: none registered yet.`。
- 重新运行 `cargo fmt --all`：通过。
- 重新运行 `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- 重新运行 `cargo test --workspace --all-targets`：通过。
- 运行 `cargo fmt --all -- --check`：通过。
- 已在 `TODO.md` 将 `M3.1 Tool 抽象` 标记为 `[DONE]` 并补充完成记录。
- 已检查 git status、diff、暂存 diff 和最近提交；变更均属于 `M3.1`。
- 本次任务已完成验证并准备提交；提交完成后停止。
