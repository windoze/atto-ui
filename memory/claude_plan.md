# Claude 执行计划

## 当前调用

- 目标：只完成 `TODO.md` 中第一个未完成任务，然后停止。
- 当前任务：`M2.3 SSE parser`。
- 计划状态：`M2.3` 已实现、验证、更新 `TODO.md` 并提交。

## 执行计划

1. 读取 `TODO.md`，识别第一个标题未带 `[DONE]` 的任务。
2. 只检查与该任务直接相关的最近提交和代码上下文。
3. 在 `crates/atto-agent-app/src/deepseek.rs` 实现 `M2.3`：新增支持分片输入的状态化 SSE parser 和完整缓冲解析入口。
4. 为 `data:` chunk、`[DONE]`、`reasoning_content`、`finish_reason`、error JSON、多行 data、分片输入、注释/未知字段忽略、malformed JSON 补单测。
5. 按顺序运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
6. 在 `TODO.md` 将当前任务标题标记为 `[DONE]` 并更新完成记录。
7. 提交本任务所有变更，然后停止，不进入下一任务。

## 进度日志

- 初始计划已创建。
- 已读取 `TODO.md`，确认首个未完成任务为 `M2.3 SSE parser`。
- 已检查最近 Git 上下文；最新提交为 `[M2.2] Add DeepSeek request models`，未声明会改变 M2.3 范围的未完成事项。
- 已复核 `PLAN.md`、`TUI_AGENT.md` 和 `deepseek.rs`；M2.3 范围限定为协议层 SSE 解析，不包含 UI 映射或网络 client 集成。
- 已在 `deepseek.rs` 实现 `ChatCompletionSseParser`、`parse_chat_completion_sse` 和 `parse_chat_completion_sse_data`。
- 已补充 content/reasoning/finish、`[DONE]`、多行 `data:`、分片输入、错误片段和 malformed JSON 单测。
- 已运行 `cargo test -p atto-agent-app deepseek`，目标 deepseek 单测全部通过。
- 已运行 `cargo fmt --all`，格式化完成。
- 已运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过。
- 已运行 `cargo test --workspace --all-targets`，通过。
- 已运行 `cargo fmt --all -- --check`，通过。
- 已在 `TODO.md` 将 `M2.3 SSE parser` 标记为 `[DONE]` 并填写完成记录。
- 已提交任务变更，提交为 `a552749 [M2.3] Add DeepSeek SSE parser`。
