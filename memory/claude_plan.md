# Claude 执行计划

## 范围

- 以 `TODO.md` 为权威任务列表。
- 本轮只完成第一个标题未带 `[DONE]` 的任务：`T13 — chat 工具调用块（消费 disclosure）（C.2）`。
- 完成 T13、更新记录并提交后停止，不处理 `R13` 或后续任务。

## 步骤

1. 读取 `TODO.md`，确认第一个未完成任务。
2. 只检查与当前任务直接相关的最近提交和工作区状态，避免开放式历史问题排查。
3. 检查 chat 消息模型、store、列表渲染、动态组件解析和 core `Disclosure` API。
4. 新增 `ChatMessageContent::ToolCall { name, status, output }`，其中 status 覆盖 running/done/error。
5. 提供工具输出流式追加与状态更新 API，避免空 delta 或同值更新产生无效 dirty 通知。
6. 在 chat 消息列表中复用 core `Disclosure` 渲染工具调用块，不在 chat 内重复实现折叠逻辑。
7. 补充单测与 PTY：覆盖 running→done/error 状态变化、输出追加、折叠和展开。
8. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
9. 验证通过后，将 T13 标题标记为 `[DONE]` 并补充完成记录。
10. 只提交本轮 T13 相关文件和计划记录。

## 进度

- 已确认第一个未完成任务为 `T13 — chat 工具调用块（消费 disclosure）（C.2）`。
- 已检查相关实现：chat 行 key 已支持文本流式 delta 不重建整行；core `Disclosure` 已支持绑定 title/status/content、键盘和鼠标折叠。
- 已实现 `ChatToolCallStatus`、`ChatMessageContent::ToolCall`、`ChatMessage::tool_call`。
- 已实现 `ChatMessageStore::append_tool_delta`、`update_tool_output`、`set_tool_status`。
- 已让 `ChatMessageList` 使用 core `Disclosure` 渲染工具调用块，并将工具状态映射到 `DisclosureStatus`。
- 已让工具调用 row key 忽略工具输出和工具状态，从而保留折叠状态并避免流式输出重建整行。
- 已补充动态消息 `tool_call` 序列化/解析与 round-trip 单测。
- 已新增 `snapshot_chat_app --tool-call` 和 PTY 覆盖 running→done→error、输出追加、折叠/展开。
- 已修正 `--tool-call` fixture 分支，避免工具调用模式仍追加默认 seed 消息导致工具块被自动滚出视口。
- 验证通过：`cargo fmt`；`cargo test -p atto-ui-chat`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`，将 T13 标记为 `[DONE]` 并记录完成内容与验证结果。
