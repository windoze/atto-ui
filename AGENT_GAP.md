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
| `Compact` | 上下文压缩进度/摘要（状态、前后 token、summary） |
| `Artifact` | 工件锚点（Code/Diff/File） |

元数据 `ChatMessageMeta`：timestamp / model / token usage / elapsed_ms / stop_reason。

### 流式与状态 (`store.rs`)
- `append_text_delta` / `append_tool_output` 增量更新
- `set_turn_status` / streaming 标志（Streaming/Complete/Failed/Canceled）
- 流式取消按钮 (`StreamingCancelButton`)
- 工具状态机：Pending/Running/Done/Error/Canceled

### 交互决策
- 工具审批 `ApprovalRequest` + `ApprovalOption`（含 allow/deny action 与 once/always/project level）
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
- 斜杠命令补全、`@` 文件/资源提及补全、排队状态行、Esc 流式中断、多行粘贴规整

---

## 二、对照 Claude Code 的能力缺口

### A. 输入区交互（缺口最大，最影响"像不像"）

- [x] **A1. 斜杠命令补全** — P2 已落地：`/` 触发命令菜单，支持宿主注入、过滤、选择、插入/提交回调，并同步运行时/JS/React。
- [x] **A2. @ 文件/资源提及补全** — P2 已落地：输入 `@` 弹出文件/资源补全，provider 回调提供候选，确认后替换当前 token。
- [x] **A3. 输入排队 & Esc 中断语义** — P4 已落地：流式中提交进入 FIFO 队列，流式完成后按 Enter 出队；未消费 Esc 走统一取消入口并置 `Canceled`。
- [x] **A4. 多行编辑增强** — P4 已落地：多行/bracketed paste 规整为 LF，裁剪尾部空白行，保持正文缩进与光标/草稿同步。

### B. 渲染保真度

- [x] **B1. 代码块语法高亮** — P1 已落地：`atto-ui-markdown` 采用 `syntect` 纯 Rust regex 路线，为 fenced code block 提供多语言高亮，未知语言回退纯文本。
- [ ] **B2. 图片/多模态** — 仍明确不在当前计划范围内；无 kitty/sixel/iterm graphics 协议支持，`Attachment` 暂不内联显示图片。
- [x] **B3. diff 语法高亮** — P1 已落地：chat diff payload 复用同一高亮器，且 +/- 增删语义色优先保留。

#### P1.0 语法高亮方案选型

结论：P1.1/P1.2 采用 **syntect**，但必须显式关闭默认特性并启用纯 Rust 正则路径，例如在
`crates/atto-ui-markdown` 中使用 `syntect = { version = "5.3", default-features = false, features = ["default-syntaxes", "regex-fancy"] }`。
不启用 `default-onig`、`regex-onig`、`default-themes` 或 `html`。终端样式由 atto-ui theme 映射，
不直接使用 syntect 内置 theme。

| 方案 | 体积/编译时间 | `#![forbid(unsafe_code)]` 兼容性 | 语言覆盖 | 结论 |
| --- | --- | --- | --- | --- |
| syntect | 中到高：会引入默认 Sublime 语法集与高亮状态机，比自研 tokenizer 重，但只需一个高亮依赖。 | 默认特性是 `default-onig`，会走 oniguruma 路线；必须关闭默认特性并选 `regex-fancy`，避免 C/onig 构建路径，项目自身无需写 unsafe。 | 高：默认语法集覆盖 Rust、JS/TS、Python、Shell、JSON、TOML/YAML、Markdown 等常见 fenced code。 | **选用**：静态代码块不需要增量 AST，syntect 的覆盖面和成熟度最符合 chat 渲染需求。 |
| tree-sitter | 高：核心依赖外还需要每种语言的 grammar crate 与 highlights query；语言越多依赖和编译时间线性增加。 | 仓库 editor crate 已有 tree-sitter，但新增语言 grammar 通常包含生成的 C parser/FFI 构建路径；不适合作为 markdown/chat 的默认轻量渲染依赖。 | 中：单语言质量高，但只覆盖显式打包的 grammar；要达到 chat 常见语言覆盖需维护多套 grammar/query。 | 不选：适合编辑器增量解析，不适合一次性渲染多语言代码块。 |
| 轻量自研 tokenizer | 低：可做到零或极少依赖，编译最快。 | 最容易保持项目代码无 unsafe。 | 低到中：只能覆盖手写的少数语言和 token 类，维护成本会随语言增长转嫁到项目内。 | 不选：视觉保真和语言覆盖不足，容易让 P1.1/P1.2 落成窄特例。 |

接口草案：

- 在 `atto-ui-markdown` 增加 `syntax` 模块，封装 syntect，不让 chat 直接依赖 syntect 类型。
- 入口以 fence info string 的第一个词或文件扩展名作为 `LanguageHint`；未知、空 hint 或 syntect 匹配失败时返回 `None`，调用方继续走纯文本路径。
- 高亮输出使用项目内中立结构，例如 `HighlightedLine { plain: String, spans: Vec<HighlightedSpan> }` 与 `HighlightedSpan { text: String, class: SyntaxClass }`；渲染阶段再把 `SyntaxClass` 映射到 `ratatui::style::Style`，避免绑定 syntect theme。
- `CodeBlockState` 保留纯文本 `plain` 用于宽度计算和水平滚动，同时保存同一行的 highlighted spans；渲染时按显示宽度切分 spans，保持现有代码块水平滚动、垂直滚动和中文/宽字符行为。
- 对 diff，`atto-ui-chat` 通过 markdown crate 暴露的同一高亮器按路径/扩展名推断语言，只高亮 diff payload；`+`/`-`/`@@`/文件头等语义前缀与现有增删行样式由 chat 渲染层最后合成，合成策略必须保证 diff 的语义前景/背景不被语法样式覆盖。

### C. 会话管理

- [x] **C1. 消息编辑/重发/回退** — P3 已落地：store 支持 `truncate_from` / `fork_at`，user 编辑重发与 assistant retry/regenerate 均会截断旧分支。
- [x] **C2. 历史搜索** — P5 已落地：`Ctrl+R` 会话内搜索、命中高亮、上一处/下一处跳转并支持屏外命中滚入视口。
- [x] **C3. Turn 级折叠 / 引用回复** — P5 已落地：turn header 支持折叠/展开，占位显示隐藏块数；turn/block 可附加为下一条输入引用并移除。

### D. 细节层

- [x] **D1. 工具权限层级** — P6 已落地：`ApprovalOption`/`ApprovalResolution` 携带 action/level，渲染区显示一次/始终/项目级/拒绝选项，回调 payload 同步层级。
- [x] **D2. 上下文压缩块** — P6 已落地：新增独立 `CompactBlock` / `CompactStatus`，渲染压缩状态、前后 token、节省量和摘要，并同步 dynamic/Node/core/React。

---

## 三、优先级建议（按投入产出排序）

| 优先级 | 项目 | 改动位置 | 收益 |
| --- | --- | --- | --- |
| 1 | **B1 代码块语法高亮** | markdown crate | 视觉收益最大，改动集中 |
| 2 | **A1/A2 斜杠命令 + @文件补全** | input.rs 新增 overlay 菜单组件 | 输入体验核心 |
| 3 | **C1 消息编辑/回退/重发** | store 加截断-fork API | 会话交互关键 |
| 4 | **B2 图片内联渲染** | 依赖终端 graphics 协议 | 工作量大，收益视场景 |

其余项（A3/A4、B3、C2/C3、D1/D2）作为二阶段补齐。
