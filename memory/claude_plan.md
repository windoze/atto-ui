# Claude 执行计划

## 当前调用

- 目标：只完成 `TODO.md` 中第一个未完成任务，然后停止。
- 当前任务：`M2.4 流式 UI 映射`。
- 任务范围：将 DeepSeek stream 的 `content` delta 写入 assistant `TextBlock`，将 `reasoning_content` delta 写入 `ThinkingBlock`，并在 stream 结束时设置 turn status 和 meta。

## 执行计划

1. 读取 `TODO.md`，识别第一个标题未带 `[DONE]` 的任务。
2. 检查最近提交是否声明与该任务直接相关的未完成事项。
3. 定向检查 `atto-agent-app` 的 app action/turn loop、`deepseek.rs` SSE 结构，以及 `atto-ui-chat` 的 block/store API。
4. 新增 DeepSeek stream 到 UI action 的映射层，保持网络 client 和错误映射留给后续 M2 任务。
5. 在 app action handler 中支持 reasoning delta，按需创建默认折叠的 `ThinkingBlock`，并在 turn 完成时写入 `ChatMessageMeta`。
6. 让现有 mock stream 通过 DeepSeek-style content/done event 进入同一映射路径，避免实现只被单测覆盖。
7. 补充单元测试覆盖 content/reasoning/meta/status 映射。
8. 按顺序运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
9. 将 `TODO.md` 中当前任务标题标记为 `[DONE]`，填写完成记录和验证命令。
10. 提交本任务所有相关变更，然后停止。

## 进度日志

- 初始执行计划已在运行项目命令前记录。
- 已确认首个未完成任务为 `M2.4 流式 UI 映射`。
- 最新提交为 `[M2.3] Record SSE parser execution status`，未声明与 M2.4 直接相关的未完成事项。
- 代码检查结果：`ChatMessageStore::append_text_delta` 已支持 `TextBlock` 和 `ThinkingBlock`，缺口在 app 层 DeepSeek stream UI 映射。
- 已新增 `stream_ui` mapper，将 DeepSeek chunk 的 `content`、`reasoning_content` 和 `[DONE]` 转成主线程 UI action。
- 已新增 reasoning delta action，主线程会在目标 assistant turn 中惰性插入默认折叠的 `ThinkingBlock`，并保持 block streaming 状态随 turn status 同步。
- 已扩展 turn done action，使完成时写入 `ChatMessageMeta`，包含 model、usage 和 stop_reason。
- 已将现有 mock turn 改为构造 DeepSeek-style content/finish/done event，并通过同一 mapper 进入 UI 更新路径。
- 已补充单元测试 `deepseek_stream_events_map_reasoning_content_and_completion_meta`，覆盖 reasoning/content 追加、block 顺序、meta、status 和 streaming reset。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已更新 `TODO.md`，将 `M2.4 流式 UI 映射` 标记为 `[DONE]` 并填写完成记录。
