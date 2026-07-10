# 执行计划

说明：本文件记录可审计的执行计划与进度更新，不包含私密推理过程。

## 当前计划

1. 读取 `TODO.md`，只识别第一个标题未以 `[DONE]` 标记的任务。
2. 检查该任务的要求、依赖、验证方式和完成记录；必要时查看 `PLAN.md` 与最近提交是否直接影响该任务。
3. 在不进行无关历史问题扫查的前提下，定位该任务涉及的代码与测试。
4. 实现首个未完成任务；如遇阻塞当前任务的具体缺陷或规格不匹配，优先修复，或在 `TODO.md` 中插入最小必要前置任务并停止。
5. 按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试；若仅文档变更且已有可复用绿灯结果，则记录跳过原因。
6. 将完成的任务标题加上 `[DONE]`，更新其完成记录；仅在阶段计划变化时更新 `PLAN.md`。
7. 提交所有本轮相关改动，提交信息包含任务编号与简明说明。
8. 完成一个任务后停止，不继续下一个任务。

## 进度

- 已创建初始执行计划，下一步读取 `TODO.md` 确认首个未完成任务。
- 已读取 `TODO.md`，首个未完成任务是 `M6.3 工具输出预算`。
- 已检查最近提交 `4468b8f [M6.2] Record completion status`，未发现与 M6.3 直接相关的未完成事项。
- 已完成 `ContextBuilder` 工具结果预算实现：默认每条 role=`tool` 消息最多 16 KiB，按 UTF-8 边界截断并追加模型可见说明；UI transcript 中的 `ToolResultBlock` 保持完整。
- 已新增定向单测覆盖长工具输出截断、UI 输出不被修改、UTF-8 边界安全；`cargo test -p atto-agent-app context_builder` 已通过且无警告。
- 已完成最终验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`、`cargo test -p atto-agent-app context_builder` 均通过。
- 已将 `TODO.md` 中 `M6.3 工具输出预算` 标记为 `[DONE]` 并补充完成记录。
- 下一步检查 diff/status 并提交。

## M6.3 执行步骤

1. 阅读 `PLAN.md` / 相关设计文档中对工具输出预算、上下文构建和 UI 展示的要求。
2. 定位工具执行结果写入 UI、`ToolResultBlock` 数据结构、transcript 到 DeepSeek `role=tool` 消息转换的实现。
3. 设计并实现工具输出预算：模型上下文中的 tool output 必须按确定性字节预算截断；UI 侧继续保留完整输出，或在必要时展示明确的尾部窗口和截断说明。
4. 补充或更新单测，覆盖短输出不截断、长输出回传模型截断、UI 保留/展示行为、UTF-8 边界安全和错误/非文本输出边界。
5. 运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
6. 在 `TODO.md` 中将 M6.3 标记为 `[DONE]` 并写入完成记录和验证命令。
7. 检查 git diff/status，提交本轮所有相关改动后停止。

## 历史记录

- 上轮已完成 `M6.2 文件 mention`：在 `ContextBuilder` 中补齐 `@path` 解析、workspace 内文件读取、UTF-8 安全截断、单文件 32 KiB / 总计 128 KiB 预算和 `<context_files>` 注入。
- 上轮已将普通 DeepSeek request 和 plan request 构建入口接入 `config.workspace` 的 file mention expansion。
- 上轮已新增单测覆盖正常注入、越界/缺失文件错误记录、预算限制和 request 构建入口。
- 上轮验证通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-agent-app context_builder`、`cargo test -p atto-agent-app deepseek_request_from_transcript_injects_file_mentions_from_config_workspace`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 上轮提交：`3e46fce [M6.2] Implement file mention context`；后续 `4468b8f [M6.2] Record completion status` 记录了 M6.2 完成状态。
