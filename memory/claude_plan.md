执行计划

1. 先读取 `TODO.md`，按文件顺序找到第一个标题未以 `[DONE]` 开头的任务，并只处理这一项。
2. 检查最近提交和当前工作区状态，确认是否有与当前任务直接相关的未完成事项或既有未提交变更；不做无关历史问题扫查。
3. 阅读当前任务涉及的需求、约束、验证要求和依赖；必要时读取相关源码、测试和文档来建立最小充分上下文。
4. 若发现当前任务被具体前置问题阻塞，按要求在 `TODO.md` 插入最小前置任务并停止；否则完整实现当前任务。
5. 修改过程中保持本文件同步，记录关键进展、计划调整、阻塞或验证结果。
6. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，随后在需要时运行完整测试套件；若发现未调度失败，修复或在 `TODO.md` 安排前置任务。
7. 完成后在 `TODO.md` 中给当前任务标题加 `[DONE]`，更新完成记录；仅当阶段计划确实变化时更新 `PLAN.md`。
8. 提交所有与本次任务相关的变更，提交信息包含任务编号和清晰描述，然后停止，不继续下一项任务。

进度记录

- 已建立初始执行计划，下一步读取 `TODO.md` 确认第一项未完成任务。
- 已读取 `TODO.md`，第一项未完成任务为 `M4.6 权限隔离`。本次只处理该任务：确认 skill 的 `tools` 字段仅作为工具偏好参与请求/展示，不改变 `ToolPermissionPolicy`，并验证 `run_command` 等副作用工具仍走审批。
- 下一步检查最近提交与工作区状态，随后阅读 skill、tool、DeepSeek request 和审批相关代码。
- 已检查最近提交和工作区：最近提交为 `[M4.5] Add skill prompt injection`，未声明相关未完成事项；当前未提交变更仅为本计划文件。
- 已阅读 `skill.rs`、`tool.rs`、`lib.rs`、`deepseek.rs` 及 `TUI_AGENT.md`。实现方向：将 loaded skill 的 `tools` 字段作为 `<skill>` 元数据里的模型可见偏好渲染，不改变 `ToolRegistry`、`ToolPermissionPolicy` 或项目级授权；新增测试覆盖声明 `run_command` 偏好的 skill 仍不能绕过审批。
- 已实现初版：`build_skill_prompt_entry` 会在 skill 声明工具偏好时输出 `tools="..."` 元数据；新增 skill prompt 单测和 app 层权限隔离单测。下一步按要求运行格式化、lint 和测试。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-agent-app tool_preferences`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。下一步更新 `TODO.md` 的 M4.6 完成记录并提交。
- 已更新 `TODO.md`：`M4.6 权限隔离` 标题已加 `[DONE]`，完成记录和验证命令已补齐。下一步提交本轮变更并停止。
