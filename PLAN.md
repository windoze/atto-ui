# 执行计划：Agent 会话视图(atto-ui-chat 重构)

本计划是 [`CHAT_UI.md`](CHAT_UI.md) 设计文档的落地步骤。目标:把 `crates/atto-ui-chat`
从「通用聊天气泡列表」重构为「agent 会话视图」,能力基准对齐 **Claude Code 功能集**(不追求外观一致)。

设计细节、数据结构、`file:line` 现状地图、序列化新格式等以 `CHAT_UI.md` 为准;本文件只给**做什么、按什么顺序、怎么验收**。

## 原则

- **小步可编译**:每个阶段结束都要 `cargo build` 通过、`cargo test` 全绿、`cargo clippy --workspace --all-targets -- -D warnings` 无告警、`cargo fmt --all -- --check` 通过(CI 同款,见 `.github/workflows/ci.yml`)。
- **每个可见改动配 PTY 快照测试**:扩展 `crates/atto-ui-chat/src/bin/snapshot_chat_app.rs` + 新增 `tests/`(参考主库 `tests/pty_*.rs` 与 `PtyTestHost`)。
- **模型先行**:P1/P2 是卡脖子重构,其余功能挂在新模型上;不要在旧 `ChatMessageContent` 四选一模型上加功能。
- **运行时同步**:任何模型变更同帧更新 `src/dynamic.rs` 序列化与 schema,并在该阶段末同步 Node/React 侧类型(`crates/atto-ui-node`、`packages/core`、`packages/react`,见 `docs/NODE_API.md`)。
- **过渡兼容**:`parse_message_value` 保留旧形(`content`/`markdown`/`tool_call`/`file`/`artifact`、`sender`、`status:"in_progress"`)解析,包成单 block 的新消息,避免一次性破坏调用方;新代码只产出新形。

## 阶段划分

### P1 — 模型地基(阻塞项)
对应 `CHAT_UI.md` §3、§7、§8。把"一条消息一种内容"改成"一条消息含有序 block"。

- `src/message.rs`:替换为新模型——`ChatMessage{ id, role, blocks, status, meta }`、`ChatRole`(去掉 `Tool` 角色)、`ChatBlock`(Text/Thinking/ToolUse/ToolResult/Diff/Todo/Attachment/Notice/Artifact)、`ChatBlockId`、`ChatTurnStatus`、`ChatError`、`ChatMessageMeta`、`ApprovalRequest`。
- `src/store.rs`:加 `next_block_id`、`append_block`、`with_block`(只读不 clone)、按 `ChatBlockId` 的 `append_text_delta`/`append_tool_output`/`set_tool_status`、`upsert_tool_result`(按 `call_id`)、`set_turn_status`、`set_meta`;保留"值未变不发脏"。
- `src/dynamic.rs`:`message_to_value`/`parse_message_value` 改为新形(§8 的 JSON 形),保留旧形兼容;round-trip 单测。
- 渲染暂时"每 block 一行",与旧外观大致持平,保证编译与现有 PTY 测试可调整通过。
- **验收**:模型/store/序列化单测;`snapshot_chat_app` 能渲染含多 block 的回合。

### P2 — 回合分组 + 性能 + 滚动
对应 `CHAT_UI.md` §4、§5(1–6、8–10)。

- `src/list.rs`:行粒度从"消息"改为"回合头 + 各 block";`ChatRowKey`(`Header{message_id}` / `Block{message_id,block_id,kind_tag}`),沿用"易变字段不进 key"。
- **去全量 clone**:行只持 `ChatBlockId`,经 store `with_block` 读取;加块级脏版本,行仅在自身 block 变化时 re-sync(替换现 `sync_body_bindings` 的 `messages.get()` 深拷贝,list.rs:529)。
- 滚动修复:构造即滚底(预载会话)、消除一帧延迟(draw 前用上帧尺寸滚动)、prepend 保锚点、`scroll_to_bottom()` 公开方法。
- 响应式换行(Text 换行宽度=气泡内容宽度,resize 重算)+ 代码/diff 区开水平滚动(去掉强制 `Never`)。
- 回合 header 只渲一次;去进行中双重指示;`ChatTimestampDivider` 用 `UnicodeWidthStr::width`。
- **验收**:长会话不卡(无每行全量 clone);PTY 覆盖自动跟随+回看、prepend 锚点、resize 换行。

### P3 — 工具语义
对应 `CHAT_UI.md` §3(ToolUse/ToolResult)、§4.2、§5(7)。

- ToolUse 行:`Disclosure` 标题=name + 入参渲染(`ToolInput::Text`→单行/代码;`Json`→key/value 列表)+ 状态图标。
- ToolResult 行:`Ansi`→ANSI SGR 上色解析;`Markdown`→`MarkdownViewer`;`Diff`→复用 `viewer.rs::diff_line_style`;超长默认尾部 N 行 + "展开全部"。
- 按 `call_id` 把 use+result 相邻配对;result 缺失显示"等待中"。
- **验收**:PTY 覆盖带入参的工具调用、流式工具输出、超长输出折叠。

### P4 — inline 审批 + inline diff
对应 `CHAT_UI.md` §6。

- 审批:`ToolUseBlock.approval` 在折叠区内渲染可聚焦选项(复用 `RadioGroup`/按钮);`ChatMessageList::on_approve`;store `resolve_approval`。
- inline diff:`DiffBlock` / `ToolOutput::Diff` inline 着色 + Accept/Reject;`on_edit_decision`;store `set_edit_decision`。
- **验收**:PTY 覆盖审批选择(含 always)、diff accept/reject 后状态锁定。

### P5 — agent 工作流类型
对应 `CHAT_UI.md` §3(Thinking/Todo/Notice/Meta/Error)。

- Thinking(默认折叠 `Disclosure` + dim)、Todo(自绘 `[ ]/[~]/[x]`)、Notice(level 着色)、回合 meta(模型/用量/耗时/停止原因)渲染、`ChatError` 结构化展示。
- store `set_todo` 等。
- **验收**:PTY 覆盖 thinking 折叠、todo 状态更新、错误展示。

### P6 — 逐条交互
对应 `CHAT_UI.md` §5(6)、§6.3。

- `ChatMessageList::on_message_action`(Copy/Retry/Regenerate/EditUser/CopyBlock)、`on_cancel(message_id)` 中断、回底入口。
- 目标 block 可聚焦 + 响应复制快捷键(文本选择为后续增强)。
- **验收**:PTY 覆盖复制、retry/regenerate 回调、流式中断置 `Canceled`。

### P7 — 规模
对应 `CHAT_UI.md` §5(8)。

- 长会话虚拟化收尾:屏外行不构建重型子视图;超大日志压测。
- **验收**:数百条(含大量工具调用)会话流畅。

## 依赖关系

- P1 → P2 → P3 → (P4, P5 并行) → P6 → P7。
- P1/P2 是阻塞项;P3 起的功能均依赖新 block 模型。
- 每个触及 `src/message.rs`/`src/dynamic.rs` 的阶段都要在该阶段末同步 Node/React 侧。

## 验证

- 每阶段:`cargo build` / `cargo test`(含 PTY) / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check`。
- 关键视觉项用 `snapshot_chat_app` 抓屏人工比对。
- 涉及 JS 侧的阶段,跑 `npm run smoke --prefix examples/react-tsx` 与 `packages/core` 的 runtime 兼容测试(见 `docs/NODE_API.md`)。

## 历史

UI 对齐(Turbo Vision)阶段的 PLAN/TODO/UI_GAPS 已归档至 [`docs/archive/2026-06-10-ui-gaps/`](docs/archive/2026-06-10-ui-gaps/)。
