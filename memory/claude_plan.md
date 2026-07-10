# 执行计划

> 说明：本文件记录可审计的执行计划、关键决策和进度更新；不包含私有推理链。

## 范围

- 以 `TODO.md` 为任务顺序和验收要求的唯一来源。
- 本轮只完成第一个未标记 `[DONE]` 的任务。
- 完成或阻塞当前任务后停止，不处理后续任务。
- 不在选择当前任务前做开放式历史问题排查。

## 当前任务

- 第一个未完成任务：`M6.1 ContextBuilder`。
- 任务要求：将 UI transcript 转成 DeepSeek messages，正确处理 user、assistant、tool use、tool result、notice、compact、skills。
- 最近提交：`[M5.R] Complete plan mode review`，未声明与 `M6.1 ContextBuilder` 直接相关的未完成事项。

## 执行步骤

1. 读取 `TODO.md`，确认第一个标题未标记 `[DONE]` 的任务。
2. 检查最近提交是否有与当前任务直接相关的未完成事项。
3. 只审查 `M6.1` 相关的 `PLAN.md`、`TUI_AGENT.md` 要求和当前 transcript 到 DeepSeek request 的构造路径。
4. 新增 `context::ContextBuilder`，集中负责 UI transcript 到 DeepSeek/OpenAI-compatible messages 的转换。
5. 保持现有 `deepseek_*_from_transcript*` 公开 API 不变，内部改为委托 ContextBuilder。
6. 添加聚焦单测，覆盖 user/assistant/tool use/tool result/notice/compact 转换和 loaded skill prompt 注入。
7. 运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
8. 验证通过后，将 `M6.1 ContextBuilder` 在 `TODO.md` 标记为 `[DONE]` 并填写完成记录。
9. 检查 diff/status/log，提交本轮相关变更，然后停止。

## 进度

- 已读取 `TODO.md` 并确认当前任务为 `M6.1 ContextBuilder`。
- 已检查最近提交，无直接相关未完成事项。
- 已新增 `atto_agent_app::context::ContextBuilder`。
- 已将现有 DeepSeek transcript request/message helper 改为委托 ContextBuilder。
- 已添加 ContextBuilder 单测，覆盖 transcript block 转换和 skill 注入。
- `cargo fmt --all` 已通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- `cargo test --workspace --all-targets` 已通过。
- `cargo fmt --all -- --check` 已通过。
- `TODO.md` 已将 `M6.1 ContextBuilder` 标记为 `[DONE]` 并记录完成情况和验证命令。
- 下一步：提交 `TODO.md`、`memory/claude_plan.md`、`crates/atto-agent-app/src/lib.rs` 和 `crates/atto-agent-app/src/context.rs`。
