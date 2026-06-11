# 执行计划

> 说明：这里记录可审计的执行计划、决策依据和进度，不记录私有推理链。

## 通用流程

1. 读取 `TODO.md`，只定位第一个标题未带 `[DONE]` 的任务，不做开放式历史问题扫描。
2. 查看最近提交信息，只有在其明确提到与当前任务直接相关的未完成问题时才纳入当前任务或作为前置任务写入 `TODO.md`。
3. 阅读当前任务涉及的源码、测试和文档，确认任务要求、依赖和验证方式。
4. 若发现当前任务被缺失功能、规格不匹配或未安排的失败测试阻塞，按要求在 `TODO.md` 中新增最小前置任务并提交后停止。
5. 以最小正确改动实现当前任务，不绕开规格要求。
6. 为新增或变更行为补充必要测试。
7. 分阶段运行格式化、lint 和测试：先 `cargo fmt`，再 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行完整测试套件。
8. 将当前任务标题前缀更新为 `[DONE]`，并填写完成记录和验证结果。
9. 仅在阶段计划本身变化时更新 `PLAN.md`。
10. 检查 `git status`、`git diff`、最近提交记录，确认只提交本次相关变更。
11. 提交 Git commit，提交信息包含任务编号和简要说明。
12. 停止，不继续下一个任务。

## 历史记录：P1.1

- 已创建本计划文件，下一步读取 `TODO.md` 定位首个未完成任务。
- 已读取 `TODO.md`：首个未完成任务为 `P1.1 新消息模型`。
- 已查看最近提交：`3623f2b Add agent chat UI design doc and plan; archive UI-gaps docs`，未声明与 `P1.1` 直接相关的未完成事项。
- 下一步读取 `CHAT_UI.md` §3 以及 `src/message.rs`/相关引用，实施新消息模型。
- 已确认影响范围：`message.rs`、`lib.rs`、`list.rs`、`store.rs`、`dynamic.rs`、chat 示例/快照，以及 editor 快照中对旧 `ChatSender` 等类型的引用。
- 实施策略：完成 `P1.1` 的新消息 envelope、`ChatRole`、`ChatBlockId`、`ChatTurnStatus`、错误/meta/usage/stop reason；为现有单内容渲染和旧 store 方法添加过渡适配，完整内容块类型矩阵留给后续 `P1.2`。
- 已完成 Rust 侧首轮改造并通过 `cargo check -p atto-ui-chat --all-targets`。下一步运行格式化、lint 和测试。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 下一步更新 `TODO.md` 的 `P1.1` 标题和完成记录，然后检查 diff 并提交。
- 已更新 `TODO.md`：`P1.1` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 下一步检查工作区状态/diff/最近提交，确认提交范围后创建任务提交。
- 已检查提交前状态：本次提交将包含 `P1.1` 相关 Rust/TODO/计划文件，并按用户要求包含已变更的 `PROMPT.md`；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。

## 历史记录：P1.2 内容块类型

- 已读取 `TODO.md`：首个未完成任务为 `P1.2 内容块类型`。
- 已查看最近提交：`e28ca50 [P1.1] Implement new chat message model`，未声明与 `P1.2` 直接相关的未完成事项。
- 实施范围：补齐 `crates/atto-ui-chat/src/message.rs` 的剩余 block variants/types，从 `src/lib.rs` 公开导出，并调整现有过渡期 `list.rs`/`dynamic.rs` match 分支以保持编译通过。
- 已实现 `Thinking`、`Diff`、`Todo`、`Notice` block 类型和 `ToolOutput::Diff`，新增 `ThinkingBlock`、`DiffData`/`EditDecision`、`TodoBlock`/`TodoItem`/`TodoState`、`NoticeBlock`/`NoticeLevel`。
- 已为完整 block id 覆盖、thinking streaming 状态同步、diff tool output 文本更新补充单元测试。
- 首次 `clippy` 失败原因：过渡渲染不再使用 `ChatMessage::first_text` 的非测试路径导致 dead code。已改为复用该 helper，同时保留 thinking block 支持。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`：`P1.2` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交包含 P1.2 相关 Rust/TODO/计划文件；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
- 下一步创建任务提交并停止。

## 历史记录：P1.3 store 改造

- 已读取 `TODO.md`：首个未完成任务为 `P1.3 store 改造`。
- 已查看最近提交：`1cf224b [P1.2] Add chat content block types`，未声明与 `P1.3` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/store.rs` 的新 block/message store API、脏通知语义与单测；不推进 P1.4/P1.5。
- 已检查 `store.rs`、`message.rs` 与 `CHAT_UI.md` 相关章节；当前 store 仍使用按 `ChatMessageId` 的文本/工具流式更新，需改为按 `ChatBlockId`。
- 实施策略：保留基础集合操作，新增块级 API；移除/替换旧 delta 调用点；给 reactive `Property` 增加只读访问能力以实现不克隆的 `with_block`。
- 已完成首轮实现：`ChatMessageStore` 增加 `next_block_id`、`append_block`、`with_block`、按 `ChatBlockId` 的文本/工具/审批/diff/todo 更新 API，并同步 `chat_demo` 与 `snapshot_chat_app` 的旧调用点。
- 首次 `clippy` 失败原因：P1.3 删除旧 store 调用后，`ChatMessage` 的旧可变 helper 只剩测试间接使用而触发 dead code。已改 list 单测直接操作 block 并删除这些旧 helper。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P1.3` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交将包含 P1.3 相关 Rust/TODO/计划文件；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
- 下一步创建任务提交并停止。

## 历史记录：P1.4 序列化新形 + 旧形兼容

- 已读取 `TODO.md`：首个未完成任务为 `P1.4 序列化新形 + 旧形兼容`。
- 已查看最近提交：`080c0dd [P1.3] Rewrite chat message store`，未声明与 `P1.4` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/dynamic.rs` 的消息序列化/解析和单测；不推进 P1.5 渲染过渡或 P1.6 JS 侧同步。
- 已检查 `CHAT_UI.md` §8、`dynamic.rs` 和 `message.rs`：需要输出新形 `{id,role,status,meta?,blocks:[...]}`，并保留旧形顶层 `content`/`markdown`/`tool_call`/`file`/`artifact` 兼容解析。
- 实施策略：新增 block 级序列化/解析 helper，覆盖 meta、失败状态、工具输入输出、审批、diff/todo/notice/artifact 等字段；旧形解析保留 `sender`、`status:"in_progress"` 与派生 block id。
- 已完成实现：`message_to_value` 现在只输出 `role/status/meta?/blocks` 新形；`parse_message_value` 优先解析新形 `blocks`，否则走旧形兼容路径。
- 已补充单测：新形输出断言、完整多 block round-trip、旧形顶层 `content`/`markdown`/`file`/`artifact`/`tool_call` 解析兼容。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P1.4` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交将包含 P1.4 相关 `TODO.md`、`memory/claude_plan.md`、`crates/atto-ui-chat/src/dynamic.rs`；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
- 下一步创建任务提交并停止。

## 历史记录：P1.5 渲染过渡(每 block 一行)

- 已读取 `TODO.md`：首个未完成任务为 `P1.5 渲染过渡(每 block 一行)`。
- 已查看最近提交：`d14da67 [P1.4] Implement chat message serialization`，未声明与 `P1.5` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/list.rs` 和 `crates/atto-ui-chat/src/bin/snapshot_chat_app.rs` 的渲染过渡；不推进 P1.6 JS 侧同步或 P2 回合头拆分。
- 已检查 `list.rs`：当前实现每条消息只生成一个行 key，并只渲染第一个非 `ToolResult` block，工具输出通过 `ToolUse` 行间接显示。
- 实施策略：将 row key 过渡为 block 级，空消息保留消息级占位；按消息内 `blocks` 顺序生成行；`ToolResult` 单独渲染为 disclosure 行；保持文本/工具输出 delta 不进入 key，避免流式更新频繁重建。
- 已完成实现：`ChatMessageList` 现在为每个 block 生成独立行，`Text`/`Thinking` 继续使用 Markdown 渲染，`ToolUse` 和 `ToolResult` 分别渲染为 disclosure，工具输出绑定到 `ToolResult` block 刷新。
- 已更新 `snapshot_chat_app`：默认种子数据包含一个由 `Text`、`Thinking`、`Notice` 组成的多 block assistant 回合。
- 首次 `clippy` 失败原因：P1.5 后旧 `ChatMessage::first_*` helper 不再使用，且一个嵌套 `if` 可折叠。已删除 stale helper 并修正 lint。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P1.5` 标题已加 `[DONE]`，完成记录和验证结果已写入。

## 历史记录：P1.6 运行时/JS 侧同步

- 已读取 `TODO.md`：首个未完成任务为 `P1.6 运行时/JS 侧同步`。
- 已查看最近提交：`8b690fc [P1.5] Render chat blocks as rows`，未声明与 `P1.6` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-node`、`packages/core`、`packages/react` 和 `docs/NODE_API.md` 的 JS/TS 侧同步；不推进 P2 渲染结构任务。
- 已确认 Rust `crates/atto-ui-chat/src/dynamic.rs` 已使用新消息形：`{id, role, status, meta?, blocks}`，block 用 `type` 和 `block_id` 区分；旧形仅作为解析兼容。
- 已发现工作区既有未提交变更：`crates/atto-ui-node/index.js` 已修改，且有未跟踪脚本 `notification.sh`、`run_agent.sh`。这些不是本次计划产生的变更；除非验证显示它们阻塞当前任务，否则不修改或回退。
- 已更新 `packages/core/src/builders.ts` 与对应 CommonJS 文件，使 chat builder 产出新 block 形，并补充 Text/Thinking/ToolUse/ToolResult/Diff/Todo/Attachment/Notice/Artifact block 构造器。
- 已同步 core builder 与类型测试中的 chat 期望。
- 已同步 React：新增 `ChatMessageList` wrapper、raw JSX `chatMessageList`/`chatmessagelist` 类型、host type 映射，并在 React 类型测试中覆盖新消息 builder。
- 已同步 `crates/atto-ui-node/index.d.ts` 的 raw chat value 类型声明，并更新 `docs/NODE_API.md` 的 chat 段，说明新 block-based shape 和 builders。
- 验证已通过：`npm run typecheck --prefix packages/core`、`node packages/core/__test__/builders.cjs`、`npm run typecheck --prefix packages/react`、`npm run build --prefix packages/react`、`npm run test:runtime --prefix packages/core`、`npm run smoke --prefix examples/react-tsx`、`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`、`npm test --prefix packages/core`、`npm test --prefix packages/react`。
- 后续调整 `ChatToolCallMessage` 的默认 turn status：`pending`/`running` 生成 `streaming`，其他工具状态生成 `complete`；已重跑 `npm run typecheck --prefix packages/core`、`node packages/core/__test__/builders.cjs`、`npm test --prefix packages/core`、`npm run typecheck --prefix packages/react`、`npm run smoke --prefix examples/react-tsx`。
- 已更新 `TODO.md`：`P1.6` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 下一步检查提交前状态/diff/最近提交，确认提交范围后创建任务提交并停止。
