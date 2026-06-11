# TODO：Agent 会话视图(atto-ui-chat 重构)

执行计划见 [`PLAN.md`](PLAN.md),设计与现状地图见 [`CHAT_UI.md`](CHAT_UI.md)。
编号 `Pn.m` 对应 PLAN 的阶段。所有改动均针对 `crates/atto-ui-chat`,除非另注。

通用验收(每条任务完成都要满足):`cargo build` / `cargo test`(含 PTY) /
`cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check` 全过(CI 同款)。

## 阶段 P1 — 模型地基(阻塞项)

参考 `CHAT_UI.md` §3(目标模型)、§7(store API)、§8(序列化)。

- [ ] **P1.1 新消息模型** — `src/message.rs`:把 `ChatMessageContent` 四选一替换为 `ChatMessage{ id, role: ChatRole, blocks: Vec<ChatBlock>, status: ChatTurnStatus, meta: ChatMessageMeta }`。新增 `ChatRole`(User/Assistant/System/Custom,**删除 Tool 角色**)、`ChatBlockId`、`ChatTurnStatus`(Streaming/Complete/Failed(ChatError)/Canceled)、`ChatError{kind,message,detail}` + `ChatErrorKind`、`ChatMessageMeta{timestamp,model,usage,elapsed_ms,stop_reason}`、`TokenUsage`、`StopReason`。保留 `Identifiable`(Id=ChatMessageId)。字段定义照搬 `CHAT_UI.md` §3。
- [ ] **P1.2 内容块类型** — `src/message.rs`:`enum ChatBlock { Text, Thinking, ToolUse, ToolResult, Diff, Todo, Attachment, Notice, Artifact }`,各结构体含 `ChatBlockId`。`ToolUseBlock{call_id,name,input:ToolInput,status:ToolStatus,approval:Option<ApprovalRequest>,collapsed}`;`ToolResultBlock{call_id,ok,exit_code,output:ToolOutput,collapsed}`;`ToolInput{Text|Json}`、`ToolOutput{Ansi|Markdown|Diff}`、`DiffBlock{path,diff:DiffData,decision:EditDecision}`、`TodoBlock`/`TodoItem`/`TodoState`、`AttachmentBlock`、`NoticeBlock`/`NoticeLevel`、`ArtifactBlock`、`ApprovalRequest`/`ApprovalOption`。保留现有 `Artifact`/`ArtifactId`/`ArtifactKind`(viewer 仍用)。
- [ ] **P1.3 store 改造** — `src/store.rs`:按新模型重写。新增 `next_block_id`、`append_block(msg,block)->ChatBlockId`、`with_block<R>(id, f)`(只读借用,**不 clone**)、`append_text_delta(block,delta)`、`append_tool_output(block,delta)`、`set_tool_status(block,status)`、`upsert_tool_result(call_id,result)`、`set_turn_status(msg,status)`、`set_meta(msg,meta)`、`resolve_approval`、`set_edit_decision`、`set_todo`;旧的按 `ChatMessageId` 的 delta 方法改为按 `ChatBlockId`。保持"值未变不发脏通知"约定。补单测(沿用现有测试风格)。
- [ ] **P1.4 序列化新形 + 旧形兼容** — `src/dynamic.rs`:`message_to_value`/`parse_message_value` 改为 `CHAT_UI.md` §8 的 `{id,role,status,meta?,blocks:[...]}` 形;block 以 `type` 区分。`parse_message_value` 保留旧形解析(顶层 `content`/`markdown`/`tool_call`/`file`/`artifact`、`sender`、`status:"in_progress"`)→ 包成单 block 新消息。新形 round-trip 单测 + 旧形解析单测。
- [ ] **P1.5 渲染过渡(每 block 一行)** — `src/list.rs`:先让新模型可渲染——把回合的 blocks 顺序渲染为行,外观与旧版大致持平,保证编译与 PTY 测试可调整通过。`snapshot_chat_app`(`src/bin/snapshot_chat_app.rs`)更新为构造含多 block 的回合。
- [ ] **P1.6 运行时/JS 侧同步** — 更新 `crates/atto-ui-node`、`packages/core`(chat builders,见 `packages/core/src/builders.ts`)、`packages/react` 的 chat 相关 TS 类型/构造器以匹配新 block 形;更新 `docs/NODE_API.md` 的 chat 段。跑 `npm run smoke --prefix examples/react-tsx` 与 core runtime 兼容测试。

## 阶段 P2 — 回合分组 + 性能 + 滚动

参考 `CHAT_UI.md` §4、§5(1–6、8–10)。

- [ ] **P2.1 行模型改为「回合头 + 各 block」** — `src/list.rs`:扁平化为行序列;`enum ChatRowKey { Header{message_id}, Block{message_id,block_id,kind_tag} }`,`kind_tag` 排除易变的 markdown/output(沿用现 `ChatMessageRowKey` 的"易变字段不进 key"思想,list.rs:398)。回合 header 仅在该回合第一可见块前渲一次(解决重复 header)。
- [ ] **P2.2 去除每行全量 clone(性能头号)** — `src/list.rs`:行只持 `ChatBlockId`,经 store `with_block` 读取自身块,**不再** `self.messages.get()` 深拷贝整个 Vec(替换 `sync_body_bindings`,list.rs:529)。加块级脏版本,行仅在自身 block 变化时 re-sync。
- [ ] **P2.3 块→控件映射** — `src/list.rs`:按 `CHAT_UI.md` §4.2 表渲染各 block(Text→`MarkdownViewer`;Thinking→折叠 `Disclosure`+dim;ToolUse/ToolResult→`Disclosure`;Diff→`viewer.rs::diff_line_style` inline;Todo→自绘;Notice/Attachment→`Text`;Artifact→现有 `ArtifactLink`)。
- [ ] **P2.4 滚动修复** — `src/list.rs`:① 构造时若已有消息且 `auto_scroll` 则置 `pending_scroll_to_bottom`(预载滚底);② 消除一帧延迟(draw 前用上帧尺寸滚动,替换 `apply_pending_scroll` 在 draw 末尾的时机,list.rs:307);③ prepend 后按新插入高度补偿 `scroll_y` 保锚点(配合 `maybe_trigger_load_more`,list.rs:169);④ 公开 `scroll_to_bottom()`。补 PTY 覆盖。
- [ ] **P2.5 响应式换行 + 代码横向滚动** — `src/list.rs`:Text/Markdown 换行宽度=布局得到的气泡内容宽度(resize 重算),去掉写死 `wrap_width=72`(list.rs:26);代码/diff/工具输出区允许水平滚动,去掉强制 `horizontal_scrollbar(Never)`(list.rs:72)。
- [ ] **P2.6 杂项渲染修复** — `src/list.rs`:去进行中双重指示(`" ▍"` 后缀 list.rs:27 与 `Spinner "Generating"` list.rs:684 二择一);`ChatTimestampDivider`(list.rs:708)改用 `UnicodeWidthStr::width`,agent 场景默认弱化逐条时间戳、强调回合边界。

## 阶段 P3 — 工具语义

参考 `CHAT_UI.md` §3(ToolUse/ToolResult)、§4.2、§5(7)。

- [ ] **P3.1 ToolUse 入参渲染** — `src/list.rs`:`Disclosure` 标题=name;`ToolInput::Text`→单行/代码,`ToolInput::Json`→key/value 列表;显示状态图标(Pending/Running/Done/Error/Canceled)。
- [ ] **P3.2 ToolResult 渲染 + use/result 配对** — `src/list.rs`:`Ansi`→ANSI SGR 上色解析,`Markdown`→`MarkdownViewer`,`Diff`→`diff_line_style`;按 `call_id` 把 use+result 相邻配对,result 缺失显示"等待中"。
- [ ] **P3.3 超长输出尾部窗口** — `src/list.rs`:`ToolOutput::Ansi` 超长默认只显示尾部 N 行 + "展开全部",避免撑爆列表。补 PTY 覆盖带入参调用、流式工具输出、超长折叠。

## 阶段 P4 — inline 审批 + inline diff

参考 `CHAT_UI.md` §6。

- [ ] **P4.1 inline 审批** — `src/list.rs` + `src/store.rs`:`ToolUseBlock.approval` 折叠区内渲染可聚焦选项(复用 `RadioGroup`/按钮),`resolved=Some` 时锁定;`ChatMessageList::on_approve`(`ApprovalDecision{message_id,block_id,approval_id,option_id}`);store `resolve_approval` 并推进 `ToolStatus`。`dynamic.rs` 加 `approve(Map)` 事件。
- [ ] **P4.2 inline diff + Accept/Reject** — `src/list.rs` + `src/store.rs`:`DiffBlock`/`ToolOutput::Diff` inline 着色 + `decision==Pending` 时 Accept/Reject;`on_edit_decision`;store `set_edit_decision`;`dynamic.rs` 加 `edit_decision(Map)` 事件。补 PTY 覆盖审批(含 always)与 diff 决策锁定。

## 阶段 P5 — agent 工作流类型

参考 `CHAT_UI.md` §3(Thinking/Todo/Notice/Meta/Error)。

- [ ] **P5.1 Thinking / Notice 渲染** — `src/list.rs`:Thinking 默认折叠 `Disclosure`+dim 流式;Notice 按 `NoticeLevel` 着色单行。
- [ ] **P5.2 Todo 面板** — `src/list.rs` + `src/store.rs`:自绘 `[ ]/[~]/[x] text`;store `set_todo` 更新。
- [ ] **P5.3 回合 meta + 错误展示** — `src/list.rs`:回合 header 渲染 model/usage/elapsed/stop_reason;`ChatTurnStatus::Failed(ChatError)` 结构化展示(kind+message+detail)。补 PTY 覆盖 thinking 折叠、todo 状态更新、错误展示。

## 阶段 P6 — 逐条交互

参考 `CHAT_UI.md` §5(6)、§6.3。

- [ ] **P6.1 逐条/回合操作** — `src/list.rs`:`on_message_action`(`MessageAction{message_id,kind}`,kind ∈ Copy/Retry/Regenerate/EditUser/CopyBlock(block_id))。
- [ ] **P6.2 中断 + 回底** — `src/list.rs`:对 `Streaming` 回合暴露 `on_cancel(message_id)`;`!follow_tail` 时提供回底入口(宿主调用 `scroll_to_bottom()`)。
- [ ] **P6.3 复制目标块** — `src/list.rs`:目标 block(代码/命令/正文)可聚焦并响应复制快捷键(文本选择留后续)。补 PTY 覆盖复制、retry/regenerate 回调、流式中断置 `Canceled`。

## 阶段 P7 — 规模

参考 `CHAT_UI.md` §5(8)。

- [ ] **P7.1 长会话虚拟化** — `src/list.rs`:屏外行不构建重型子视图,只构建可见窗口内的行;数百条(含大量工具调用)会话压测流畅。

## 收尾

- [ ] **收尾 1 — 全量校验** — `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets` 全过;JS 侧 `npm run smoke --prefix examples/react-tsx` 与 core runtime 兼容测试通过。
- [ ] **收尾 2 — 快照人工比对** — 用 `snapshot_chat_app` 抓屏,逐项核对 `CHAT_UI.md` §2 能力矩阵从 ❌/⚠️ 转为 ✅。
