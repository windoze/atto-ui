# CHAT_UI.md — Agent 会话视图设计文档

本文件是 `crates/atto-ui-chat` 从「通用聊天气泡列表」重构为「agent 会话视图」的设计蓝本。
基准参考 **Claude Code 所需的 UI 功能集**(只对齐*能力*,不要求外观一致——本库与 Claude Code
是两套不同的 UI 设计)。文档力求自包含:实现者读完后无需反复在代码库中搜索即可动手。

> 术语:本文中 **block(内容块)** 指一条消息内的一个可独立渲染单元(文本/思考/工具调用/工具结果/diff/todo…);
> **turn(回合)** 指一条 `ChatMessage`(一个角色的一次发言,可含多个 block)。

---

## 1. 现状代码地图(改造前)

crate 路径:`crates/atto-ui-chat`,包名 `atto-ui-chat`,库入口 `atto_ui_chat`。依赖 `atto-ui`(主库,路径依赖)、
`atto-ui-markdown`(`MarkdownViewer`)。

| 文件 | 行数 | 职责 |
|---|---|---|
| `src/lib.rs` | 26 | 模块声明 + 公开导出 |
| `src/message.rs` | 282 | 数据模型:`ChatMessage` / `ChatMessageContent` / `ChatSender` / 状态枚举 / `Artifact` |
| `src/store.rs` | 398 | `ChatMessageStore`:基于 `Property<Vec<ChatMessage>>` 的增改、流式 delta、状态更新 |
| `src/list.rs` | 1093 | `ChatMessageList` 控件:渲染、`ForEachIdentifiable` 行复用、自动滚动/加载更多 |
| `src/input.rs` | 1025 | `ChatInputPanel` / `ChatInputHandle` / `ChatInputMode`(Text/Choice/Confirm/Custom) |
| `src/panel.rs` | 69 | `ChatPanel`:上 list + 下 input 的 `VStack` 组合 |
| `src/viewer.rs` | 206 | `ArtifactViewer` / `TextArtifactViewer`:把 artifact 在独立窗口打开,diff 着色 |
| `src/dynamic.rs` | 610 | 运行时桥接:消息 ↔ `ComponentValue` 序列化、schema、动态组件注册(供 Node/React 用) |
| `src/bin/snapshot_chat_app.rs` | 386 | PTY 测试用快照应用 |

### 1.1 当前数据模型(`src/message.rs`)

```rust
pub struct ChatMessageId(pub u64);          // message.rs:92

pub enum ChatSender {                        // message.rs:118
    User, Assistant, System, Tool(String), Custom(String),
}
// alignment(): User => Right, 其余 => Left  (message.rs:138)

pub enum ChatMessageStatus { Final, InProgress, Failed(String) }  // message.rs:146
pub enum ChatToolCallStatus { Running, Done, Error }              // message.rs:153

pub enum ChatMessageContent {                // message.rs:160  —— 关键:四选一
    Text { markdown: String },
    File { name: String, url: Option<String> },
    ToolCall { name: String, status: ChatToolCallStatus, output: String },
    Artifact { kind: ArtifactKind, anchor: ArtifactId, title: String },
}

pub struct ChatMessage {                     // message.rs:181
    pub id: ChatMessageId,
    pub sender: ChatSender,
    pub timestamp: Option<String>,
    pub status: ChatMessageStatus,
    pub content: ChatMessageContent,         // 一条消息只能有一种内容
}
// 构造器: ChatMessage::{text, file, tool_call, artifact}; with_timestamp / with_status
// 实现了 atto_ui::composable::Identifiable(Id = ChatMessageId),供 ForEach 用
```

### 1.2 当前 store(`src/store.rs`)

`ChatMessageStore { messages: Property<Vec<ChatMessage>>, next_id: Arc<AtomicU64> }`。
方法:`binding()` / `messages()` / `replace_all()` / `next_message_id()` / `push` / `prepend` /
`prepend_many` / `update_message` / `set_status` / `update_text` / `append_delta` /
`update_tool_output` / `append_tool_delta` / `set_tool_status`。
所有改写走 `Property::update_if`,**值未变则不发脏通知**(已有测试覆盖)。

### 1.3 当前 list 控件(`src/list.rs`)关键机制

- `ChatMessageList { messages: Binding<Vec<ChatMessage>>, row_keys: Binding<Vec<ChatMessageRowKey>>,
  list: ForEachIdentifiable<ChatMessageRowKey, ChatMessageRow>, config, … }`(list.rs:50)。
- 每条消息 → 一个 `ChatMessageRow`。行视图:`VStack`(可选时间戳分隔线 + 对齐气泡)。
- 气泡布局:`HStack`,气泡 `Size::Weight(3)` + `Spacer` `Size::Weight(1)`,按 `sender.alignment()` 左/右放(list.rs:641)。
- 气泡内:header(sender 标签)+ body + (InProgress 时)`Spinner "Generating"`(list.rs:667)。
- body 按内容类型分支(list.rs:760 `ChatMessageBody`):Text→`MarkdownViewer`(`wrap_width=72` 写死,
  `horizontal_scrollbar(Never)`),File→`VStack` 文本,ToolCall→`Disclosure`(可折叠,状态映射),Artifact→`ArtifactLink`(一行下划线链接,点开回调)。
- **行键 `ChatMessageRowKey`**(list.rs:398)刻意**忽略** Text 的 markdown、ToolCall 的 output/status,
  使流式 delta 不会重建行(已有测试 list.rs:1030)。内容**类型/标识**变化才换行。
- **流式正文同步**:`ChatMessageRow::sync_body_bindings()`(list.rs:529)每帧把 store 里的最新文本/工具输出
  写进行内 `Binding`。⚠️ 它调用 `self.messages.get()` **深拷贝整个 Vec** 再线性查找,且被 4 个 Layout 方法 + draw + event 各调一次。
- **自动滚动**:`auto_scroll` + `follow_tail`;`track_message_changes()`(list.rs:192)用 `DirtyObserver`
  检测消息变化,置 `pending_scroll_to_bottom`;`apply_pending_scroll()`(list.rs:221)在 `draw` 末尾(`list.draw` 之后)滚到底。
- **加载更多**:`on_load_more` + `maybe_trigger_load_more()`(list.rs:169),滚动到顶(`scroll_y==0`)且 armed 时回调。
- 动态属性:`messages / spacing / padding / wrap_width / show_timestamps / auto_scroll`(list.rs:239)。

### 1.4 运行时桥接(`src/dynamic.rs`)—— 改模型必须同步改这里

- 序列化:`messages_to_component_value` / `parse_messages_value`(dynamic.rs:60)。
- 单条消息编码为 `Map{ id:U64, sender:String, timestamp:String|Null, status, content }`。
  - sender 字符串:`"user"|"assistant"|"system"|"tool:<name>"|"custom:<name>"`(dynamic.rs:92)。
  - status:`"final"|"in_progress"` 或 `Map{ failed: String }`(dynamic.rs:102)。
  - content:`Map{ markdown }` | `Map{ file:{name,url} }` | `Map{ tool_call:{name,status,output} }` |
    `Map{ artifact:{kind,anchor,title} }`(dynamic.rs:114)。
- schema:`chat_message_list_schema()`(dynamic.rs:47)事件 `load_more`、`open_artifact(String)`,`allow_children(false)`。
- 注册:`register_chat_message_list` / `register_chat_input_panel` / `register_runtime_components()`(幂等)。
- **影响面**:任何模型变更都要更新 `message_to_value` / `parse_message_value` / schema,
  并同步 Node 包(`crates/atto-ui-node`)/ React 包(`packages/react`、`packages/core`)对应的 TS 类型与构造器(参考 `docs/NODE_API.md`)。

### 1.5 输入侧(`src/input.rs`)

- `ChatInputMode`:`Text(cfg)` / `Choice(cfg)` / `Confirm(cfg)` / `Custom`(input.rs:107)。
- `ChatInputResponse`:`Text(String)` / `Choice { index, label }` / `Custom(String)`(input.rs:132)。
- `ChatInputHandle`:持有 mode/draft/custom/history/selection/enabled/clear_on_submit 等 `Binding`;`panel()` 产出 `ChatInputPanel`。
- `ChatPanel`(panel.rs):固定布局 = `list`(Weight 1) + `input`(Content)。**确认/选择提示在这里,与具体工具调用无关联。**

### 1.6 Artifact 查看器(`src/viewer.rs`)

`ArtifactViewer::open(Artifact) -> WindowId`,在桌面右上角开独立窗口;diff 行着色 `diff_line_style`(viewer.rs:179,
`@@`黄 / `+++/---`青 / `+`绿 / `-`红)。**仅在独立窗口里 inline,不在消息流内 inline。**

---

## 2. 能力差距矩阵(对照 Claude Code 功能集)

✅ 已具备 / ⚠️ 部分 / ❌ 缺失。

| 能力 | 现状 | 关键差距 |
|---|---|---|
| markdown 流式文本 | ✅ | `MarkdownViewer` + `append_delta` |
| 一回合多 block(thinking+text+多 tool_use 交错) | ❌ | `ChatMessageContent` 四选一;一回合被拆成多条消息 |
| thinking / reasoning 块(可折叠) | ❌ | 无类型 |
| 工具**入参** input | ❌ | `ToolCall` 只有 name/status/output |
| tool_use 与 tool_result 分离 | ❌ | 合并为一个 `output:String` |
| 工具输出 ANSI / 超长尾部窗口 | ❌ | 纯 String,无 ANSI/截断 |
| 工具属于 assistant 回合 | ❌ | 被建模成 `sender=Tool(name)` 独立气泡 |
| 工具三态 + 可折叠 | ✅ | `ChatToolCallStatus` + `Disclosure` |
| inline 审批(挂在 tool_use,支持 always) | ❌ | 审批是底部独立面板,与调用脱节 |
| inline diff + Accept/Reject | ❌ | diff 仅在侧栏 viewer,链接式 |
| Todo/任务进度面板 | ❌ | 无 |
| Plan 模式(展示+接受) | ❌ | 无 |
| 子 agent / Task 嵌套 | ❌ | 无层级 |
| 系统/上下文压缩通知 | ⚠️ | 只有 `ChatSender::System` |
| 回合元数据(模型/用量/耗时/停止原因) | ❌ | 仅 `Option<timestamp>` |
| 错误清晰区分(API/工具/限流/拒答) | ⚠️ | 只有 `Failed(String)` + 工具 `Error` |
| 复制消息/代码、文本选择 | ❌ | body 不可聚焦不可选 |
| retry / regenerate / 编辑重发 | ❌ | 无逐条操作 |
| 中断/取消生成(Esc) | ❌ | 只有 spinner |
| 自动跟随 + 跳到最新 | ⚠️ | 有 follow_tail;有一帧延迟、预载不滚底、prepend 不保锚点、无回底入口 |
| 响应式换行 / 代码横向滚动 | ❌ | `wrap_width=72` 写死 + 禁水平滚动 |
| 长会话虚拟化 + 去全量 clone | ❌ | 每行每帧深拷贝整个 Vec,无窗口化 |

---

## 3. 目标模型(重构核心)

把「一条消息一种内容」改为「**一条消息 = 有序 block 列表**」,并以 turn 为渲染分组单位。
以下为建议数据结构(`src/message.rs`),标注 *proposed*,字段名可微调但语义应保留。

```rust
// ---- 标识 ----
pub struct ChatMessageId(pub u64);
pub struct ChatBlockId(pub u64);   // 每个 block 的稳定 id:行复用 key + 定点流式更新

// ---- 角色 / 回合 ----
pub enum ChatRole { User, Assistant, System, Custom(String) }
// 注意:不再有 Tool 角色。工具调用是 Assistant 回合内的 block。

pub enum ChatTurnStatus {
    Streaming,            // 生成中(替代旧 InProgress)
    Complete,             // 完成(替代旧 Final)
    Failed(ChatError),    // 整个回合失败
    Canceled,             // 被用户中断
}

pub struct ChatError {
    pub kind: ChatErrorKind,   // Api / Tool / RateLimit / Refusal / Network / Other
    pub message: String,
    pub detail: Option<String>, // 堆栈/退出码等
}
pub enum ChatErrorKind { Api, Tool, RateLimit, Refusal, Network, Other }

pub struct TokenUsage { pub input: u64, pub output: u64 }
pub enum StopReason { EndTurn, MaxTokens, ToolUse, StopSequence, Refusal }

pub struct ChatMessageMeta {
    pub timestamp: Option<String>,
    pub model: Option<String>,
    pub usage: Option<TokenUsage>,
    pub elapsed_ms: Option<u64>,
    pub stop_reason: Option<StopReason>,
}

pub struct ChatMessage {
    pub id: ChatMessageId,
    pub role: ChatRole,
    pub blocks: Vec<ChatBlock>,
    pub status: ChatTurnStatus,
    pub meta: ChatMessageMeta,
}

// ---- 内容块 ----
pub enum ChatBlock {
    Text(TextBlock),
    Thinking(ThinkingBlock),
    ToolUse(ToolUseBlock),
    ToolResult(ToolResultBlock),
    Diff(DiffBlock),
    Todo(TodoBlock),
    Attachment(AttachmentBlock),   // 替代旧 File
    Notice(NoticeBlock),           // 系统/压缩通知
    Artifact(ArtifactBlock),       // 保留:跳侧栏的链接(沿用现有 viewer.rs)
}

pub struct TextBlock     { pub id: ChatBlockId, pub markdown: String, pub streaming: bool }
pub struct ThinkingBlock { pub id: ChatBlockId, pub markdown: String, pub streaming: bool, pub collapsed: bool }

pub struct ToolUseBlock {
    pub id: ChatBlockId,
    pub call_id: String,             // 与 ToolResultBlock.call_id 对应
    pub name: String,
    pub input: ToolInput,
    pub status: ToolStatus,          // Pending/Running/Done/Error/Canceled
    pub approval: Option<ApprovalRequest>,  // Some 时 inline 渲染审批 UI
    pub collapsed: bool,
}
pub enum ToolInput { Text(String), Json(atto_ui::ComponentValue) } // Text=命令; Json=结构化参数(渲染成 key/value)
pub enum ToolStatus { Pending, Running, Done, Error, Canceled }

pub struct ToolResultBlock {
    pub id: ChatBlockId,
    pub call_id: String,             // 关联回 ToolUseBlock
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub output: ToolOutput,
    pub collapsed: bool,
}
pub enum ToolOutput { Ansi(String), Markdown(String), Diff(DiffData) }

pub struct DiffBlock {               // 独立的"建议编辑"(不一定来自工具结果)
    pub id: ChatBlockId,
    pub path: String,
    pub diff: DiffData,
    pub decision: EditDecision,
}
pub struct DiffData { pub unified: String }   // 先用 unified diff 文本;后续可结构化为 hunks
pub enum EditDecision { Pending, Accepted, Rejected }

pub struct TodoBlock { pub id: ChatBlockId, pub items: Vec<TodoItem> }
pub struct TodoItem  { pub text: String, pub state: TodoState }
pub enum TodoState { Pending, InProgress, Done }

pub struct AttachmentBlock { pub id: ChatBlockId, pub name: String, pub url: Option<String>, pub mime: Option<String> }

pub struct NoticeBlock { pub id: ChatBlockId, pub level: NoticeLevel, pub text: String }
pub enum NoticeLevel { Info, Warning, Error }

pub struct ArtifactBlock { pub id: ChatBlockId, pub kind: ArtifactKind, pub anchor: ArtifactId, pub title: String }

// ---- 审批(inline 权限提示)----
pub struct ApprovalRequest {
    pub id: String,                       // 审批请求 id(回调用)
    pub prompt: String,                   // 例如 "Run `rm -rf build`?"
    pub options: Vec<ApprovalOption>,     // 例: Allow once / Allow always / Deny
    pub resolved: Option<String>,         // Some(option_id) 表示已决,UI 锁定
}
pub struct ApprovalOption { pub id: String, pub label: String }
```

设计要点与理由:
- **`ChatBlockId` 稳定 id 是关键**:行复用 key 用它;store 的定点更新(append delta / set status)按它定位;
  虚拟化按块粒度。沿用现有"行键忽略易变字段"思想——块内的 markdown/output 流式变化不改 key,块身份/类型变化才换行。
- **去掉 `ChatSender::Tool`**:工具调用/结果是 Assistant 回合内的 block,语义归属正确,回合分组不被打散。
- **tool_use / tool_result 分离 + `call_id` 关联**:渲染时把同 `call_id` 的 use+result 相邻成对展示;result 缺失即"等待中"。
- **`ToolOutput::Ansi`**:工具日志保留 ANSI;渲染层负责解析 + 尾部窗口(见 §5)。
- **审批挂在 `ToolUseBlock.approval`**:inline,支持多选项(含 always)。底部 `ChatInputPanel` 的 Confirm/Choice 仍可保留作通用输入,但审批不再依赖它。

---

## 4. 渲染与回合分组

### 4.1 行模型
保持基于 `ForEachIdentifiable` 的行复用,但**行的粒度从「消息」改为「回合头 + 各 block」**,以便虚拟化与块级流式复用。

- 把会话**扁平化**为行序列。每个 `ChatMessage` 产出:
  1. 一个 **TurnHeader 行**(角色标签 + meta:模型/用量/耗时/时间戳/停止原因/错误);仅在回合的**第一**可见块前渲染一次。
  2. 每个 `ChatBlock` 一行。
- 行 key(`enum ChatRowKey`)建议:
  - `Header { message_id }`
  - `Block { message_id, block_id, kind_tag }`(`kind_tag` 用块的类型/身份字段,排除易变的 markdown/output)
- tool_use + tool_result 配对:可在扁平化阶段把对应 result 排到其 use 之后(若 result 在同消息或后续消息中,按 `call_id` 查表)。

### 4.2 各 block 的渲染映射(复用现有控件)

| Block | 控件 | 备注 |
|---|---|---|
| Text | `MarkdownViewer` | 换行宽度改为响应式(§5);`streaming` 时追加光标后缀 |
| Thinking | `Disclosure`(默认折叠)+ `MarkdownViewer` | 弱化样式(dim) |
| ToolUse | `Disclosure`(标题=name)+ 入参渲染(Text→单行/代码;Json→key/value 列表)+ 状态图标 + 可选审批区 | 审批区见 §6 |
| ToolResult | `Disclosure` 内容:Ansi→ANSI 解析行;Markdown→`MarkdownViewer`;Diff→diff 着色(复用 `viewer.rs::diff_line_style`) | 尾部窗口 + "展开全部" |
| Diff | inline diff 视图(复用 `diff_line_style`)+ Accept/Reject 按钮 | 见 §6 |
| Todo | 自绘:每项 `[ ]/[~]/[x] text` | 可复用主题字形 |
| Attachment | `Text` "File: name (url)" | |
| Notice | 单行带 level 着色 | |
| Artifact | `ArtifactLink`(现有,list.rs:913) | 沿用 viewer.rs 侧栏 |

### 4.3 对齐与气泡
- 现有左右对齐(User 右 / 其余左)可保留为回合级容器对齐;但**不需追求 Claude Code 外观**。
- 建议:User 回合右对齐窄气泡;Assistant/System 回合左对齐、占满宽度(便于代码/diff/日志)。
- header 在回合内只渲染一次(解决现有"每条消息重复 header"问题)。

---

## 5. 流式 / 滚动 / 性能(具体修复项)

逐项给出现状问题 + 目标行为,供实现核对:

1. **去掉每行全量深拷贝(性能头号问题)**
   - 现状:`ChatMessageRow::sync_body_bindings`(list.rs:529)`self.messages.get()` 克隆整个 `Vec`,且被 4 个 Layout 方法 + draw + event 各调一次。
   - 目标:行只持有 `ChatBlockId`,通过 store 的**只读访问器**读取自己那个 block,不克隆全量。
     建议在 store 上加 `fn with_block<R>(&self, id: ChatBlockId, f: impl FnOnce(&ChatBlock) -> R) -> Option<R>`(内部借用,不 clone)。
   - 配合**块级脏版本**:store 给每个 block 维护 version/dirty;行仅在自己的 block 变了才 re-sync,避免无谓重建 `Binding`。

2. **响应式换行宽度 + 代码横向滚动**
   - 现状:`wrap_width=72` 写死(list.rs:26),Text body `horizontal_scrollbar(Never)`(list.rs:72)。
   - 目标:Text/Markdown 的换行宽度 = 布局得到的气泡内容宽度(随终端/窗口变化,resize 时重算);
     代码块/diff/工具输出允许水平滚动(去掉强制 `Never`,或对代码区单独开 `Auto`)。

3. **初始滚动到底**
   - 现状:构造函数不置 `pending_scroll_to_bottom`,`messages_observer` 以当前消息为基线 → 预载会话首帧停在顶部。
   - 目标:构造时若已有消息且 `auto_scroll`,置 `pending_scroll_to_bottom = true`。

4. **消除自动滚动一帧延迟**
   - 现状:`apply_pending_scroll()` 在 `list.draw` 之后执行(list.rs:307),偏移下一帧才生效。
   - 目标:用上一帧的 content/viewport 尺寸在 draw **前**先滚动,或在同帧二次布局后滚动;保证新消息当帧可见。

5. **prepend 保持滚动锚点**
   - 现状:`maybe_trigger_load_more` 触发后 `store.prepend_many` 在顶部插入,`scroll_y` 数值不变 → 视口跳到不同内容。
   - 目标:prepend 后把 `scroll_y` 增加"新插入内容高度",保持用户当前可见行不动。

6. **跳到最新 + 中断**
   - `ChatMessageList` 暴露 `scroll_to_bottom()`;当 `!follow_tail`(用户上滚了)时,提供回底入口(可由宿主放置按钮/快捷键调用)。
   - 中断:列表对 `Streaming` 回合暴露 `on_cancel(message_id)` 回调,宿主据此停止生成并把回合置 `Canceled`。

7. **工具输出 ANSI + 尾部窗口**
   - `ToolOutput::Ansi` 渲染:解析 ANSI SGR 上色;超长时默认只显示尾部 N 行 + "展开全部"(避免撑爆列表)。

8. **长会话虚拟化**
   - 行已是 `ForEach`;确认/补充屏外行不构建重型子视图(按需构建可见窗口内的行)。

9. **进行中指示去冗余**
   - 现状:`" ▍"` 文本后缀(list.rs:27)+ `Spinner "Generating"`(list.rs:684)同时出现。择一(建议保留流式光标后缀,回合级状态用 header 指示)。

10. **时间戳分隔线 Unicode 修复**
    - 现状:`ChatTimestampDivider`(list.rs:708)用 `label.len()`(字节)算居中,非 ASCII 错位。
    - 目标:改用 `UnicodeWidthStr::width`;agent 场景默认弱化/隐藏逐条时间戳,改强调回合边界。

---

## 6. inline 审批 与 inline diff(接入点)

### 6.1 审批
- 数据:`ToolUseBlock.approval: Option<ApprovalRequest>`。`resolved=None` 时渲染选项;`Some(id)` 时锁定并显示结果。
- 控件:在 ToolUse 行的折叠区内,用一行可聚焦的按钮组(复用 `RadioGroup`/按钮)渲染 `options`。
- 回调:`ChatMessageList::on_approve(impl Fn(ApprovalDecision))`,`ApprovalDecision { message_id, block_id, approval_id, option_id }`。
- store:`resolve_approval(block_id, option_id)` 把 `resolved` 写入并(通常)把 `ToolStatus` 推进到 `Running`/`Canceled`。

### 6.2 inline diff
- 数据:`DiffBlock { path, diff: DiffData, decision }` 或 `ToolOutput::Diff`。
- 渲染:inline 用 `viewer.rs::diff_line_style` 着色;`decision==Pending` 时显示 Accept/Reject。
- 回调:`ChatMessageList::on_edit_decision(impl Fn(EditDecision 决策))`,携带 `message_id`/`block_id`。
- store:`set_edit_decision(block_id, EditDecision)`。

### 6.3 逐条/回合操作
- `ChatMessageList::on_message_action(impl Fn(MessageAction))`,`MessageAction { message_id, kind }`,
  `kind ∈ { Copy, Retry, Regenerate, EditUser, CopyBlock(block_id) }`。
- 复制:需让目标 block(代码/命令/正文)可聚焦并响应复制快捷键;完整文本选择由 P8 补齐。

---

## 7. store API(`src/store.rs`)变更

保持"值未变不发脏通知"的既有约定。新增/调整(*proposed*):

```rust
// 回合
fn push_message(&self, msg: ChatMessage) -> ChatMessageId;
fn set_turn_status(&self, id: ChatMessageId, status: ChatTurnStatus) -> bool;
fn set_meta(&self, id: ChatMessageId, meta: ChatMessageMeta) -> bool;

// 块(按 ChatBlockId 定点)
fn next_block_id(&self) -> ChatBlockId;
fn append_block(&self, msg: ChatMessageId, block: ChatBlock) -> Option<ChatBlockId>;
fn with_block<R>(&self, id: ChatBlockId, f: impl FnOnce(&ChatBlock) -> R) -> Option<R>; // 只读,不 clone
fn append_text_delta(&self, block: ChatBlockId, delta: &str) -> bool;     // Text/Thinking
fn append_tool_output(&self, block: ChatBlockId, delta: &str) -> bool;    // ToolResult(Ansi/Markdown)
fn set_tool_status(&self, block: ChatBlockId, status: ToolStatus) -> bool;
fn upsert_tool_result(&self, call_id: &str, result: ToolResultBlock) -> bool; // 按 call_id 关联
fn resolve_approval(&self, block: ChatBlockId, option_id: &str) -> bool;
fn set_edit_decision(&self, block: ChatBlockId, decision: EditDecision) -> bool;
fn set_todo(&self, block: ChatBlockId, items: Vec<TodoItem>) -> bool;
```

旧的 `append_delta`/`update_tool_output`/`set_tool_status`(按 `ChatMessageId`)在新模型下改为按 `ChatBlockId`。

---

## 8. 运行时序列化(`src/dynamic.rs`)新格式

消息 `Map` 新形:
```text
{
  id: u64,
  role: "user"|"assistant"|"system"|"custom:<name>",
  status: "streaming"|"complete"|"canceled" | { failed: { kind, message, detail? } },
  meta?: { timestamp?, model?, usage?:{input,output}, elapsed_ms?, stop_reason? },
  blocks: [ Block, ... ]
}
```
Block 形(以 `type` 区分):
```text
{ type:"text",      block_id:u64, markdown:String, streaming?:bool }
{ type:"thinking",  block_id:u64, markdown:String, streaming?:bool, collapsed?:bool }
{ type:"tool_use",  block_id:u64, call_id:String, name:String, input:{text:String}|{json:Value},
                    status:"pending"|"running"|"done"|"error"|"canceled",
                    approval?:{ id, prompt, options:[{id,label}], resolved? } }
{ type:"tool_result", block_id:u64, call_id:String, ok:bool, exit_code?:i64,
                      output:{ansi:String}|{markdown:String}|{diff:String} }
{ type:"diff",      block_id:u64, path:String, diff:String, decision:"pending"|"accepted"|"rejected" }
{ type:"todo",      block_id:u64, items:[{text,state:"pending"|"in_progress"|"done"}] }
{ type:"attachment",block_id:u64, name:String, url?:String, mime?:String }
{ type:"notice",    block_id:u64, level:"info"|"warning"|"error", text:String }
{ type:"artifact",  block_id:u64, kind:"code"|"diff"|"file", anchor:String, title:String }
```
schema 新增事件(`chat_message_list_schema`):`approve(Map)`、`edit_decision(Map)`、`cancel(Map)`、`message_action(Map)`,
并保留 `load_more`、`open_artifact(String)`。

**向后兼容**:建议在 `parse_message_value` 里保留对旧形(顶层 `content` / `markdown` / `tool_call` / `file` / `artifact`、
`sender`、`status:"in_progress"`)的解析,把旧 `content` 包成单 block 的新 `ChatMessage`,以免一次性破坏现有调用方;
新代码一律产出新形。Node/React 侧需同步更新 TS 类型与构造器(见 §1.4、`docs/NODE_API.md`、`packages/core/src/builders.ts` 的 chat builders)。

---

## 9. 分阶段实现计划

每阶段都应能编译、过 `cargo test`、并更新 `snapshot_chat_app` + PTY 测试。

- **P1 模型地基**:新 `message.rs`(blocks/turn/meta)+ store 改造 + `dynamic.rs` 序列化(新形 + 旧形兼容)+ round-trip 单测。渲染暂时按"每 block 一行"产出,与旧外观大致持平。
- **P2 回合分组 + 性能 + 滚动**:TurnHeader 行 + 块级行键;去全量 clone(`with_block` + 块级脏版本);初始滚底 / 一帧延迟 / prepend 锚点 / 响应式换行 / 代码横向滚动。
- **P3 工具语义**:ToolUse 入参渲染(Text/Json)+ ToolResult 分离 + `call_id` 配对 + ANSI/尾部窗口。
- **P4 审批 + diff**:inline 审批(`on_approve`/`resolve_approval`)+ inline diff + Accept/Reject(`on_edit_decision`)。
- **P5 agent 工作流类型**:Thinking、Todo、Notice、回合 meta(模型/用量/耗时/停止原因)、错误结构化展示。
- **P6 交互**:复制/选择、retry/regenerate/编辑重发(`on_message_action`)、中断(`on_cancel`)、跳到最新。
- **P7 规模**:长会话虚拟化收尾、超大日志压测。

---

## 10. 测试

- **单元测试**:`store.rs`(delta/状态/审批/diff 决策、值未变不发脏)、`dynamic.rs`(新形 + 旧形 round-trip)、
  `list.rs`(行键在流式 delta 下稳定、类型变化才换行)。
- **PTY 集成**:扩展 `src/bin/snapshot_chat_app.rs` 覆盖:多 block 回合、流式文本、工具调用(入参+结果+折叠)、
  inline 审批选择、inline diff accept/reject、自动滚动跟随 + 回看、加载更多保锚点。运行方式见 `CLAUDE.md`(`cargo test`,`PtyTestHost`)。
- **回归基准**:`cargo build`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`(CI 同款,见 `.github/workflows/ci.yml`)。

---

## 11. 不在本次范围

- 输入框富能力(@-mention、图片粘贴、slash 命令补全)——属 `input.rs`,可单列。
- artifact 侧栏查看器的增强(`viewer.rs`)——本次仅复用其 `diff_line_style`。
- 主题/配色细化——按需,不追求与 Claude Code 外观一致。
