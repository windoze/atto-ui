# AGENT_GAP.md — Chat 控件对齐 Claude Code 的能力缺口

> 目标：评估 `crates/atto-ui-chat` 现有实现，与 Claude Code (CLI) 完整能力之间的差距，并给出优先级建议。
> 评估范围：`message.rs` / `list.rs` / `input.rs` / `store.rs` / `viewer.rs` + `crates/atto-ui-markdown`。

## 一、现状：已具备的能力

### 消息模型 (`message.rs`)
`ChatBlock` 已覆盖以下块类型：

| 块类型 | 说明 |
| --- | --- |
| `Text` | Markdown 文本，支持 streaming 标志 |
| `Thinking` | 思考块，可折叠 |
| `ToolUse` | 工具调用（含 `ApprovalRequest` 审批） |
| `ToolResult` | 工具结果（Ansi / Markdown / Diff 三种输出） |
| `Diff` | 文件 diff（含 Accept/Reject 决策） |
| `Plan` | 计划块（含 Accept/Reject 决策） |
| `Task` | 子 agent 任务（可嵌套 transcript，可折叠） |
| `Todo` | 待办列表（Pending/InProgress/Done） |
| `Attachment` | 附件（名称/URL/mime） |
| `Notice` | 通知（Info/Warning/Error） |
| `Artifact` | 工件锚点（Code/Diff/File） |

元数据 `ChatMessageMeta`：timestamp / model / token usage / elapsed_ms / stop_reason。

### 流式与状态 (`store.rs`)
- `append_text_delta` / `append_tool_output` 增量更新
- `set_turn_status` / streaming 标志（Streaming/Complete/Failed/Canceled）
- 流式取消按钮 (`StreamingCancelButton`)
- 工具状态机：Pending/Running/Done/Error/Canceled

### 交互决策
- 工具审批 `ApprovalRequest` + `ApprovalOption`
- 编辑决策 Accept/Reject (`EditDecision`)
- 计划决策 Accept/Reject (`PlanDecision`)

### 渲染 (`list.rs` + markdown crate)
- pulldown-cmark markdown：标题 / 代码块 / 表格 / 列表 / 引用 / 强调 / 删除线 / 链接
- ANSI SGR 彩色工具输出 (`ansi_sgr_lines` / `apply_sgr_sequence`)
- diff +/- 行着色
- 块级折叠 (collapsed)
- 文本选择 + 复制 (`RenderedTextSelectionState`)
- 时间戳分隔线
- 虚拟滚动 + 自动跟随尾部 (auto-scroll / follow-tail)
- 响应式换行宽度

### 输入 (`input.rs`)
- 四种输入模式：Text / Choice / Confirm / Custom
- 输入历史记录 (history)
- 可绑定的草稿/自定义回复

---

## 二、对照 Claude Code 的能力缺口

### A. 输入区交互（缺口最大，最影响"像不像"）

- [ ] **A1. 斜杠命令补全** — `/` 触发命令菜单（`/clear`、`/model`、`/review`…）。当前无任何 slash/command_menu 实现。
- [ ] **A2. @ 文件/资源提及补全** — 输入 `@` 弹出文件路径补全，mention 芯片渲染。当前无。
- [ ] **A3. 输入排队 & Esc 中断语义** — 有取消按钮，但缺"流式中排队新消息 / 连按 Esc 打断"的状态机。
- [ ] **A4. 多行编辑增强** — 粘贴多行、拖入文件路径转 attachment 等。

### B. 渲染保真度

- [ ] **B1. 代码块语法高亮** — markdown crate 无 syntect/tree-sitter 依赖，fenced code 按纯文本渲染。**视觉差距最明显。**
- [ ] **B2. 图片/多模态** — 无 kitty/sixel/iterm graphics 协议支持，`Attachment` 无法内联显示图片。
- [ ] **B3. diff 语法高亮** — 目前仅 +/- 行着色，无语法层面着色。

### C. 会话管理

- [ ] **C1. 消息编辑/重发/回退** — store 只有 append/update，缺"截断到某条并 fork 重生成"。
- [ ] **C2. 历史搜索** — 会话内搜索/跳转（类 Ctrl+R）。
- [ ] **C3. Turn 级折叠 / 引用回复** — 块级折叠已有，turn 级折叠与引用未见。

### D. 细节层

- [ ] **D1. 工具权限层级** — 只有一次性 `ApprovalOption`，缺 allow-once / always / 项目级权限 UI。
- [ ] **D2. 上下文压缩块** — `NoticeLevel` 可显示提示，但无专门的 compact 进度/摘要块。

---

## 三、优先级建议（按投入产出排序）

| 优先级 | 项目 | 改动位置 | 收益 |
| --- | --- | --- | --- |
| 1 | **B1 代码块语法高亮** | markdown crate | 视觉收益最大，改动集中 |
| 2 | **A1/A2 斜杠命令 + @文件补全** | input.rs 新增 overlay 菜单组件 | 输入体验核心 |
| 3 | **C1 消息编辑/回退/重发** | store 加截断-fork API | 会话交互关键 |
| 4 | **B2 图片内联渲染** | 依赖终端 graphics 协议 | 工作量大，收益视场景 |

其余项（A3/A4、B3、C2/C3、D1/D2）作为二阶段补齐。
