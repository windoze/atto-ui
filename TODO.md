# TODO：Agent 会话视图(atto-ui-chat 重构)

执行计划见 [`PLAN.md`](PLAN.md),设计与现状地图见 [`CHAT_UI.md`](CHAT_UI.md)。
编号 `Pn.m` 对应 PLAN 的阶段。所有改动均针对 `crates/atto-ui-chat`,除非另注。

通用验收(每条任务完成都要满足):`cargo build` / `cargo test`(含 PTY) /
`cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check` 全过(CI 同款)。

## 阶段 P1 — 模型地基(阻塞项)

参考 `CHAT_UI.md` §3(目标模型)、§7(store API)、§8(序列化)。

- [x] **[DONE] P1.1 新消息模型** — `src/message.rs`:把 `ChatMessageContent` 四选一替换为 `ChatMessage{ id, role: ChatRole, blocks: Vec<ChatBlock>, status: ChatTurnStatus, meta: ChatMessageMeta }`。新增 `ChatRole`(User/Assistant/System/Custom,**删除 Tool 角色**)、`ChatBlockId`、`ChatTurnStatus`(Streaming/Complete/Failed(ChatError)/Canceled)、`ChatError{kind,message,detail}` + `ChatErrorKind`、`ChatMessageMeta{timestamp,model,usage,elapsed_ms,stop_reason}`、`TokenUsage`、`StopReason`。保留 `Identifiable`(Id=ChatMessageId)。字段定义照搬 `CHAT_UI.md` §3。
  - 完成记录（2026-06-12）：已将 Rust 消息 envelope 改为 `ChatRole` + `Vec<ChatBlock>` + `ChatTurnStatus` + `ChatMessageMeta`；删除旧 `Tool` 角色建模，旧工具调用构造器过渡为 assistant 回合内的 tool blocks；同步更新 store/list/dynamic、chat 示例/快照和 editor artifact 快照的编译引用。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P1.2 内容块类型** — `src/message.rs`:`enum ChatBlock { Text, Thinking, ToolUse, ToolResult, Diff, Todo, Attachment, Notice, Artifact }`,各结构体含 `ChatBlockId`。`ToolUseBlock{call_id,name,input:ToolInput,status:ToolStatus,approval:Option<ApprovalRequest>,collapsed}`;`ToolResultBlock{call_id,ok,exit_code,output:ToolOutput,collapsed}`;`ToolInput{Text|Json}`、`ToolOutput{Ansi|Markdown|Diff}`、`DiffBlock{path,diff:DiffData,decision:EditDecision}`、`TodoBlock`/`TodoItem`/`TodoState`、`AttachmentBlock`、`NoticeBlock`/`NoticeLevel`、`ArtifactBlock`、`ApprovalRequest`/`ApprovalOption`。保留现有 `Artifact`/`ArtifactId`/`ArtifactKind`(viewer 仍用)。
  - 完成记录（2026-06-12）：已补齐 `Thinking`、`Diff`、`Todo`、`Notice` block 类型和 `ToolOutput::Diff`，新增 `ThinkingBlock`、`DiffData`/`EditDecision`、`TodoBlock`/`TodoItem`/`TodoState`、`NoticeBlock`/`NoticeLevel`，并从 `lib.rs` 公开导出；现有过渡渲染和动态桥接补齐 match 分支以保持编译兼容，后续新形序列化和逐 block 渲染仍按 P1.4/P1.5 执行。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --workspace --all-targets` 全部通过。
- [x] **[DONE] P1.3 store 改造** — `src/store.rs`:按新模型重写。新增 `next_block_id`、`append_block(msg,block)->ChatBlockId`、`with_block<R>(id, f)`(只读借用,**不 clone**)、`append_text_delta(block,delta)`、`append_tool_output(block,delta)`、`set_tool_status(block,status)`、`upsert_tool_result(call_id,result)`、`set_turn_status(msg,status)`、`set_meta(msg,meta)`、`resolve_approval`、`set_edit_decision`、`set_todo`;旧的按 `ChatMessageId` 的 delta 方法改为按 `ChatBlockId`。保持"值未变不发脏通知"约定。补单测(沿用现有测试风格)。
  - 完成记录（2026-06-12）：已将 `ChatMessageStore` 改为消息 id + block id 双计数，新增块级 append/read/update API；`with_block` 通过 reactive `Property::with` 只读访问，避免克隆整个消息列表；文本 delta、工具输出、工具状态、tool result upsert、审批、diff 决策、todo 和 meta/turn status 更新均保持未变化不触发脏通知。同步更新 chat demo、snapshot app 和 list 单测里的旧消息级 delta/status 调用。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P1.4 序列化新形 + 旧形兼容** — `src/dynamic.rs`:`message_to_value`/`parse_message_value` 改为 `CHAT_UI.md` §8 的 `{id,role,status,meta?,blocks:[...]}` 形;block 以 `type` 区分。`parse_message_value` 保留旧形解析(顶层 `content`/`markdown`/`tool_call`/`file`/`artifact`、`sender`、`status:"in_progress"`)→ 包成单 block 新消息。新形 round-trip 单测 + 旧形解析单测。
  - 完成记录（2026-06-12）：已将 `messages_to_component_value` 输出改为新形 `role/status/meta?/blocks`，按 `type` 序列化 Text/Thinking/ToolUse/ToolResult/Diff/Todo/Attachment/Notice/Artifact block；`parse_message_value` 支持新形完整 round-trip，并保留旧形 `content`/`markdown`/`tool_call`/`file`/`artifact`、`sender`、`status:"in_progress"` 解析兼容。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P1.5 渲染过渡(每 block 一行)** — `src/list.rs`:先让新模型可渲染——把回合的 blocks 顺序渲染为行,外观与旧版大致持平,保证编译与 PTY 测试可调整通过。`snapshot_chat_app`(`src/bin/snapshot_chat_app.rs`)更新为构造含多 block 的回合。
  - 完成记录（2026-06-12）：已将 `ChatMessageList` 行 key 从消息级过渡为 block 级,按消息内 `blocks` 顺序生成一行一个 block；空消息保留消息级占位行。`Text`/`Thinking` 继续用 Markdown 渲染并支持流式 delta,`ToolUse` 与 `ToolResult` 分别渲染为 disclosure 行,工具输出更新按 `ToolResult` block 绑定刷新。`snapshot_chat_app` 默认种子数据新增一个多 block assistant 回合,PTY 工具调用测试同步改为验证独立 tool result 行折叠。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P1.6 运行时/JS 侧同步** — 更新 `crates/atto-ui-node`、`packages/core`(chat builders,见 `packages/core/src/builders.ts`)、`packages/react` 的 chat 相关 TS 类型/构造器以匹配新 block 形;更新 `docs/NODE_API.md` 的 chat 段。跑 `npm run smoke --prefix examples/react-tsx` 与 core runtime 兼容测试。
  - 完成记录（2026-06-12）：已将 `@atto-ui/core` chat builder 改为产出新 `{id, role, status, meta?, blocks}` 形，新增 Text/Thinking/ToolUse/ToolResult/Diff/Todo/Attachment/Notice/Artifact block 构造器和对应 TS 类型；`ChatTextMessage`/`ChatFileMessage`/`ChatToolCallMessage`/`ChatArtifactMessage` 便利构造器同步生成稳定 `block_id`。`@atto-ui/react` 新增 `ChatMessageList` wrapper、raw JSX 类型和 host 映射，并重导出 chat value builders/types；`@atto-ui/node` 类型声明和 `docs/NODE_API.md` 已同步新 block 形说明。
  - 验证：`npm run typecheck --prefix packages/core`、`node packages/core/__test__/builders.cjs`、`npm run typecheck --prefix packages/react`、`npm run build --prefix packages/react`、`npm run test:runtime --prefix packages/core`、`npm run smoke --prefix examples/react-tsx`、`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`、`npm test --prefix packages/core`、`npm test --prefix packages/react` 全部通过。

## 阶段 P2 — 回合分组 + 性能 + 滚动

参考 `CHAT_UI.md` §4、§5(1–6、8–10)。

- [x] **[DONE] P2.1 行模型改为「回合头 + 各 block」** — `src/list.rs`:扁平化为行序列;`enum ChatRowKey { Header{message_id}, Block{message_id,block_id,kind_tag} }`,`kind_tag` 排除易变的 markdown/output(沿用现 `ChatMessageRowKey` 的"易变字段不进 key"思想,list.rs:398)。回合 header 仅在该回合第一可见块前渲一次(解决重复 header)。
  - 完成记录（2026-06-12）：已将 chat list 行 key 拆为 `ChatRowKey::Header` 与 `ChatRowKey::Block`，`row_keys_from_messages` 现在按每个回合先产出一个 header 行、再按 block 顺序产出 block 行；block `kind_tag` 继续排除 markdown/output/status 等流式易变字段。渲染层同步拆为 header 行与 block 内容行，block 行不再重复角色 header；header 文本通过 binding 随回合状态刷新。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P2.2 去除每行全量 clone(性能头号)** — `src/list.rs`:行只持 `ChatBlockId`,经 store `with_block` 读取自身块,**不再** `self.messages.get()` 深拷贝整个 Vec(替换 `sync_body_bindings`,list.rs:529)。加块级脏版本,行仅在自身 block 变化时 re-sync。
  - 完成记录（2026-06-12）：`ChatMessageList` 现持有 `ChatMessageStore`，行构造和同步通过 store `with_message`/`with_block` 只读访问数据；block 行只保存自身 `ChatBlockId`，header 行按 message 版本同步，block 行按 block 版本同步，移除了 `sync_body_bindings` 中每行 `messages.get()` 全量 Vec clone。`ChatMessageStore` 新增 message/block 版本跟踪，文本 delta、工具输出、工具状态、turn status、审批/diff/todo、append/upsert/replace 等路径按实际变更 bump 对应版本，并补充版本范围单测。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P2.3 块→控件映射** — `src/list.rs`:按 `CHAT_UI.md` §4.2 表渲染各 block(Text→`MarkdownViewer`;Thinking→折叠 `Disclosure`+dim;ToolUse/ToolResult→`Disclosure`;Diff→`viewer.rs::diff_line_style` inline;Todo→自绘;Notice/Attachment→`Text`;Artifact→现有 `ArtifactLink`)。
  - 完成记录（2026-06-12）：已将 chat block 过渡渲染改为目标控件映射：Text 继续使用 `MarkdownViewer`，Thinking 改为弱化样式的 `Disclosure + MarkdownViewer`，ToolUse 以工具名为标题并渲染 Text/Json 入参和静态审批信息，ToolResult 按 Ansi/Markdown/Diff 分派并新增 ANSI SGR 解析与 inline diff 视图，Diff block 复用 `viewer::diff_line_style` inline 渲染，Todo 改为自绘列表，Attachment/Notice 改为 `Text`，Artifact 继续使用 `ArtifactLink`。新增 `snapshot_chat_app --block-mapping` deterministic 场景和 PTY 覆盖。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat --test pty_chat chat_block_mapping_renders_each_block_with_target_widget -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P2.4 滚动修复** — `src/list.rs`:① 构造时若已有消息且 `auto_scroll` 则置 `pending_scroll_to_bottom`(预载滚底);② 消除一帧延迟(draw 前用上帧尺寸滚动,替换 `apply_pending_scroll` 在 draw 末尾的时机,list.rs:307);③ prepend 后按新插入高度补偿 `scroll_y` 保锚点(配合 `maybe_trigger_load_more`,list.rs:169);④ 公开 `scroll_to_bottom()`。补 PTY 覆盖。
  - 完成记录（2026-06-12）：已在底层 `StackCore` 增加下一次布局后生效的滚动调整，使滚到底和 prepend 锚点补偿都在新内容尺寸计算后、子组件渲染前应用；`ChatMessageList` 现在初始已有消息时预载滚底，消息追加在同帧跟随尾部，`on_load_more` prepend 后按新增内容高度补偿 `scroll_y`，并公开 `scroll_to_bottom()`。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat messages_`、`cargo test -p atto-ui-chat --test pty_chat chat_auto_follow_pauses_after_user_scrolls_up -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_load_more_on_scroll_top -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P2.5 响应式换行 + 代码横向滚动** — `src/list.rs`:Text/Markdown 换行宽度=布局得到的气泡内容宽度(resize 重算),去掉写死 `wrap_width=72`(list.rs:26);代码/diff/工具输出区允许水平滚动,去掉强制 `horizontal_scrollbar(Never)`(list.rs:72)。
  - 完成记录（2026-06-12）：已移除 chat list 的固定默认 `wrap_width=72`，默认按布局估算气泡宽度并在实际绘制时用真实内容区宽度刷新 Markdown wrap width；保留显式 `wrap_width` 作为上限。`Text`、`Thinking` 与 Markdown 工具输出随终端/窗口宽度重算换行；默认 list scroll config 不再强制禁用横向滚动。`DiffView` 与 `AnsiOutputView` 新增水平偏移、viewport/content size 和横向滚动事件处理，Markdown 代码块继续使用既有嵌入式水平滚动。新增响应式布局 snapshot 场景、PTY 窄/宽换行覆盖，以及 diff/ANSI 横向偏移单测。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --lib`、`cargo test -p atto-ui-chat --test pty_chat chat_markdown_wraps_to_responsive_bubble_width -- --exact`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P2.6 杂项渲染修复** — `src/list.rs`:去进行中双重指示(`" ▍"` 后缀 list.rs:27 与 `Spinner "Generating"` list.rs:684 二择一);`ChatTimestampDivider`(list.rs:708)改用 `UnicodeWidthStr::width`,agent 场景默认弱化逐条时间戳、强调回合边界。
  - 完成记录（2026-06-12）：已保留 Text 流式光标后缀，同时让已有 `DisclosureStatus::Running` 的 Thinking block 不再重复追加光标；`ChatTimestampDivider` 分隔线改为基于 `UnicodeWidthStr::width` 的显示宽度计算，并补齐宽度安全截断；agent 默认隐藏逐条时间戳，回合 header 使用弱化加粗样式强调回合边界。新增默认时间戳、光标去重、Unicode 分隔线宽度单测。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。

## 阶段 P3 — 工具语义

参考 `CHAT_UI.md` §3(ToolUse/ToolResult)、§4.2、§5(7)。

- [x] **[DONE] P3.1 ToolUse 入参渲染** — `src/list.rs`:`Disclosure` 标题=name;`ToolInput::Text`→单行/代码,`ToolInput::Json`→key/value 列表;显示状态图标(Pending/Running/Done/Error/Canceled)。
  - 完成记录（2026-06-12）：已为 ToolUse disclosure 增加 `ToolUseDetailsView`，Text 入参渲染为 `Input: ...` 单行或多行缩进代码块，Json 入参渲染为 key/value 列表；ToolUse 行绑定随 block 版本同步入参/审批文本。`DisclosureStatus` 新增 Canceled 状态，ToolStatus 的 Pending/Running/Done/Error/Canceled 现在分别显示 `[ ]/[~]/[x]/[!]/[-]` 图标。snapshot chat app 与 PTY 测试覆盖 Text 入参、Json 入参、Pending/Running/Done/Error/Canceled 图标。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P3.2 ToolResult 渲染 + use/result 配对** — `src/list.rs`:`Ansi`→ANSI SGR 上色解析,`Markdown`→`MarkdownViewer`,`Diff`→`diff_line_style`;按 `call_id` 把 use+result 相邻配对,result 缺失显示"等待中"。
  - 完成记录（2026-06-12）：已在 chat row 扁平化阶段按 `call_id` 将 ToolUse 与同消息或后续消息中的首个未配对 ToolResult 相邻展示，并跳过 result 原位置重复行；缺失 result 时新增 pending result 行，标题与内容显示“等待中”并使用运行中 disclosure 状态。现有 ToolResult 的 `Ansi`/`Markdown`/`Diff` 输出继续分别走 ANSI SGR 解析、`MarkdownViewer`、inline diff 着色渲染。补充行模型单测覆盖同消息重排、后续消息配对、缺失 result 等待行，并更新 block mapping PTY 用例断言等待行。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat --test pty_chat chat_block_mapping_renders_each_block_with_target_widget -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P3.3 超长输出尾部窗口** — `src/list.rs`:`ToolOutput::Ansi` 超长默认只显示尾部 N 行 + "展开全部",避免撑爆列表。补 PTY 覆盖带入参调用、流式工具输出、超长折叠。
  - 完成记录（2026-06-12）：已为 `ToolOutput::Ansi` 的 `AnsiOutputView` 增加默认尾部窗口，超出阈值时只渲染尾部 12 行并显示 `展开全部` 提示，支持点击提示或键盘展开为完整输出；流式追加时保持尾部跟随。新增 `snapshot_chat_app --long-tool-output` 场景，覆盖带入参工具调用、流式工具输出和超长折叠/展开。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat --lib ansi_output_view_tails_long_output_until_expanded`、`cargo test -p atto-ui-chat --test pty_chat chat_tool_result_long_ansi_output_tails_streams_and_expands -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。

## 阶段 P4 — inline 审批 + inline diff

参考 `CHAT_UI.md` §6。

- [x] **[DONE] P4.1 inline 审批** — `src/list.rs` + `src/store.rs`:`ToolUseBlock.approval` 折叠区内渲染可聚焦选项(复用 `RadioGroup`/按钮),`resolved=Some` 时锁定;`ChatMessageList::on_approve`(`ApprovalDecision{message_id,block_id,approval_id,option_id}`);store `resolve_approval` 并推进 `ToolStatus`。`dynamic.rs` 加 `approve(Map)` 事件。
  - 完成记录（2026-06-12）：已新增 `ApprovalDecision` 和 `ChatMessageList::on_approve`，ToolUse 折叠区内的 unresolved approval 渲染为可聚焦按钮并触发审批回调；resolved approval 改为锁定的已选结果显示。`ChatMessageStore::resolve_approval` 现在校验 option id，并将 allow/always 类选项推进到 `Running`、deny/reject/cancel 类选项推进到 `Canceled`。`dynamic.rs` schema/注册增加 `approve(Map)` 事件，payload 包含 `message_id`、`block_id`、`approval_id`、`option_id`。`snapshot_chat_app --inline-approval` 与 PTY 覆盖了 always 审批、状态推进和锁定行为。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P4.2 inline diff + Accept/Reject** — `src/list.rs` + `src/store.rs`:`DiffBlock`/`ToolOutput::Diff` inline 着色 + `decision==Pending` 时 Accept/Reject;`on_edit_decision`;store `set_edit_decision`;`dynamic.rs` 加 `edit_decision(Map)` 事件。补 PTY 覆盖审批(含 always)与 diff 决策锁定。
  - 完成记录（2026-06-12）：已新增 `EditDecisionEvent` 与 `ChatMessageList::on_edit_decision`，`DiffBlock` pending 行 inline 显示 Accept/Reject，触发后通过 store `set_edit_decision` 更新为 accepted/rejected 并显示锁定状态；`ToolOutput::Diff` 继续复用 inline diff 着色渲染。`dynamic.rs` schema/注册新增 `edit_decision(Map)` 事件，payload 包含 `message_id`、`block_id`、`decision`。`snapshot_chat_app --inline-diff` 与 PTY 覆盖了 diff accept 后事件、状态标题更新和锁定行为；既有 inline approval(always) PTY 覆盖保持通过。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --lib diff_decision_view_emits_decision_and_locks_when_resolved`、`cargo test -p atto-ui-chat --lib edit_decision`、`cargo test -p atto-ui-chat --test pty_chat chat_inline_diff_buttons_emit_and_lock -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_inline_approval_buttons_emit_and_lock -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_block_mapping_renders_each_block_with_target_widget -- --exact`、`cargo test -p atto-ui-chat --lib`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。

## 阶段 P5 — agent 工作流类型

参考 `CHAT_UI.md` §3(Thinking/Todo/Notice/Meta/Error)。

- [x] **[DONE] P5.1 Thinking / Notice 渲染** — `src/list.rs`:Thinking 默认折叠 `Disclosure`+dim 流式;Notice 按 `NoticeLevel` 着色单行。
  - 完成记录（2026-06-12）：已补齐 Thinking 流式更新路径，`append_text_delta` 现在支持 Text/Thinking block；动态解析省略 `collapsed` 的 thinking block 时默认折叠，序列化显式保留 `collapsed=false` 以保证展开状态 round-trip。Thinking 渲染保持 `Disclosure + MarkdownViewer`、运行状态与弱化内容样式；Notice 按 Info/Warning/Error 分别使用 Cyan/Yellow/Red 单行标签。新增 `snapshot_chat_app --thinking-notice` 和 PTY 覆盖 thinking 默认折叠/展开与 Notice level 标签。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P5.2 Todo 面板** — `src/list.rs` + `src/store.rs`:自绘 `[ ]/[~]/[x] text`;store `set_todo` 更新。
  - 完成记录（2026-06-12）：Todo 面板现通过稳定 Todo 行 key + `Binding<Vec<TodoItem>>` 渲染 `[ ]/[~]/[x] text`，`ChatMessageStore::set_todo` 的块级版本更新会同步到既有 Todo 行而不重建行；保留未变化不发脏通知。新增 `snapshot_chat_app --todo-panel` 场景和 PTY 覆盖 Todo 状态更新。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat --lib todo`、`cargo test -p atto-ui-chat --test pty_chat chat_todo_panel_renders_and_updates_state -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P5.3 回合 meta + 错误展示** — `src/list.rs`:回合 header 渲染 model/usage/elapsed/stop_reason;`ChatTurnStatus::Failed(ChatError)` 结构化展示(kind+message+detail)。补 PTY 覆盖 thinking 折叠、todo 状态更新、错误展示。
  - 完成记录（2026-06-12）：回合 header 现在直接渲染绑定文本，并按行显示 `model`、`usage`、`elapsed_ms`、`stop_reason`；`ChatTurnStatus::Failed(ChatError)` 显示 failed 状态以及 `Error kind`、`Error message`、`Error detail` 结构化字段。新增 `snapshot_chat_app --turn-meta-error` 场景和 PTY 覆盖；既有滚动 PTY fixture 已按真实可见 header 高度更新。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --test pty_chat chat_turn_header_renders_meta_and_structured_error -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_auto_follow_pauses_after_user_scrolls_up -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_load_more_on_scroll_top -- --exact`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。

## 阶段 P6 — 逐条交互

参考 `CHAT_UI.md` §5(6)、§6.3。

- [x] **[DONE] P6.1 逐条/回合操作** — `src/list.rs`:`on_message_action`(`MessageAction{message_id,kind}`,kind ∈ Copy/Retry/Regenerate/EditUser/CopyBlock(block_id))。
  - 完成记录（2026-06-12）：已新增 `MessageAction` / `MessageActionKind` 和 `ChatMessageList::on_message_action`，回合 header 在设置回调时渲染 Copy/Edit 或 Copy/Retry/Regenerate 操作按钮，block 行渲染 `Copy block` 操作按钮并携带目标 `ChatBlockId`；`dynamic.rs` 同步新增 `message_action(Map)` 事件与 payload 序列化，`snapshot_chat_app --message-actions` 覆盖 Copy/Edit/Retry/Regenerate/CopyBlock 回调路径。
  - 验证：`cargo check -p atto-ui-chat --all-targets`、`cargo fmt --all`、`cargo test -p atto-ui-chat --lib message_action`、`cargo test -p atto-ui-chat --lib turn_action_specs`、`cargo test -p atto-ui-chat --test pty_chat chat_message_action_buttons_emit_turn_and_block_actions -- --exact`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P6.2 中断 + 回底** — `src/list.rs`:对 `Streaming` 回合暴露 `on_cancel(message_id)`;`!follow_tail` 时提供回底入口(宿主调用 `scroll_to_bottom()`)。
  - 完成记录（2026-06-12）：已新增 `ChatMessageList::on_cancel`，streaming 回合头在配置回调后显示 `Cancel` 控件并向宿主回传 `ChatMessageId`；`dynamic.rs` 同步新增 `cancel(Map)` 事件，payload 包含 `message_id`。新增 `is_following_tail()`，宿主可在返回 `false` 时显示回底入口并调用既有 `scroll_to_bottom()`。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat --lib cancel`、`cargo test -p atto-ui-chat --test pty_chat chat_streaming_cancel_button_emits_and_marks_turn_canceled -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。
- [x] **[DONE] P6.3 复制目标块** — `src/list.rs`:目标 block(代码/命令/正文)可聚焦并响应复制快捷键(文本选择留后续)。补 PTY 覆盖复制、retry/regenerate 回调、流式中断置 `Canceled`。
  - 完成记录（2026-06-12）：已为 chat block body 增加可聚焦复制目标包装层，点击正文/代码/命令类 block 后按复制快捷键会触发既有 `MessageActionKind::CopyBlock(block_id)`；同时补齐 `ChatMessageList`、`ChatMessageRow`、`ChatMessageBody` 的焦点委托，使 block body 能接收键盘事件。`ChatPanel` 构造时显式保留输入框为初始焦点，避免列表变为可聚焦后影响既有输入体验。PTY 覆盖已在 message action 场景验证正文复制快捷键、Retry/Regenerate 回调；既有 cancel PTY 覆盖验证流式中断置 `Canceled`。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat --lib block_copy_target_emits_copy_action_on_shortcut`、`cargo test -p atto-ui-chat --test pty_chat chat_message_action_buttons_emit_turn_and_block_actions -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_streaming_cancel_button_emits_and_marks_turn_canceled -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_input_modes_submit_callbacks -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_textarea_multiline_history_and_kill_ring -- --exact`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。

## 阶段 P7 — 规模

参考 `CHAT_UI.md` §5(8)。

- [x] **[DONE] P7.1 长会话虚拟化** — `src/list.rs`:屏外行不构建重型子视图,只构建可见窗口内的行;数百条(含大量工具调用)会话压测流畅。
  - 完成记录（2026-06-12）：`ChatMessageList` 已从 eager `ForEachIdentifiable` 切换为 `ScrollContainer` + chat 专用虚拟 `ScrollContent`；现在仅为可见窗口附近的行构建/缓存 `ChatMessageRow`，滚动时会裁剪屏外行缓存，并通过轻量行高估算 + 可见行实测高度维持滚动内容尺寸。补充长会话压测单测覆盖 300 个工具调用回合只实现可见行，并补充偏移窗口下虚拟行按钮鼠标分发测试；虚拟行同时保留 captured row，修复按钮 Down/Up 在窗口偏移布局中的事件回传。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 全部通过。

## 阶段 P8 — 能力矩阵遗留项(收尾 2 前置)

收尾 2 快照审计（2026-06-12）确认 `CHAT_UI.md` §2 仍有三个能力不能按 spec-correct 方式标为 ✅；这些任务必须在最终人工比对前完成。

- [ ] **P8.1 Plan 模式展示+接受** — 新增独立 plan block 模型和渲染,不要用 Todo/Approval 组合替代。`src/message.rs` 增加 `PlanBlock`/`PlanItem`/`PlanDecision`;`src/store.rs` 增加 plan 更新与决策 API;`src/list.rs` 渲染 plan 面板并在 pending 时显示 Accept/Reject,决策后锁定;`src/dynamic.rs`、`packages/core`、`packages/react`、`docs/NODE_API.md` 同步新形。`snapshot_chat_app --plan-mode` + PTY 覆盖展示、接受事件和锁定状态。
- [ ] **P8.2 子 agent / Task 嵌套块** — 新增显式 task/subagent block,支持在 assistant 回合内展示一个可折叠的子 agent 运行摘要和嵌套 transcript/blocks,不要把它建模成普通 tool output 文本。同步 store 定点更新、dynamic/TS 类型、React builders 和文档。`snapshot_chat_app --nested-task` + PTY 覆盖折叠/展开、嵌套内容渲染、状态更新和虚拟化下可见窗口行为。
- [ ] **P8.3 聊天文本选择** — 在 `ChatMessageList` 的文本/代码/命令目标 block 内实现真实文本选择,与已有 Copy/CopyBlock 动作并存；支持鼠标拖选和复制所选文本,选择范围跨软换行时保持显示宽度正确。补单元测试和 `snapshot_chat_app --text-selection` PTY 覆盖选区渲染、复制所选文本、未选择时仍触发 CopyBlock。

## 收尾

- [x] **[DONE] 收尾 1 — 全量校验** — `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets` 全过;JS 侧 `npm run smoke --prefix examples/react-tsx` 与 core runtime 兼容测试通过。
  - 完成记录（2026-06-12）：已完成收尾全量校验，Rust workspace 格式化、lint、构建和全量测试通过；React TSX 示例 smoke 与 core runtime Node/Bun/Deno 兼容测试通过。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`、`npm run smoke --prefix examples/react-tsx`、`npm run test:runtime --prefix packages/core` 全部通过。
- [ ] **收尾 2 — 快照人工比对** — 依赖 P8.1、P8.2、P8.3。用 `snapshot_chat_app` 抓屏,逐项核对 `CHAT_UI.md` §2 能力矩阵从 ❌/⚠️ 转为 ✅。
  - 阻塞记录（2026-06-12）：已构建 `snapshot_chat_app` 并通过临时 PTY 审计辅助程序抓取 13 组快照（default tail、block mapping top/bottom、long tool output、inline approval、inline diff、thinking/notice、todo、turn meta/error、message actions、cancel、responsive layout、artifact link）。审计确认多数矩阵项已有可见覆盖，但 `Plan 模式(展示+接受)`、`子 agent / Task 嵌套`、以及“复制消息/代码、文本选择”中的文本选择仍缺少 spec-correct 实现与快照覆盖；已新增 P8.1-P8.3 作为本任务前置。
