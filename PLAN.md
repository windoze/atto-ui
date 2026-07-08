# 执行计划：Chat 控件对齐 Claude Code 能力缺口

本计划是 [`AGENT_GAP.md`](AGENT_GAP.md) 缺口分析的落地步骤。目标：在现有 agent 会话视图
（`crates/atto-ui-chat`）基础上，补齐与 Claude Code (CLI) 之间的能力差距，使 chat 控件
能够**完整复刻** Claude Code 的核心交互与渲染能力（不追求外观逐像素一致）。

缺口清单、优先级、`file:line` 现状以 `AGENT_GAP.md` 为准；本文件只给**做什么、按什么顺序、怎么验收**。

> **范围说明**：本计划**不包含**图片/多模态内联渲染（`AGENT_GAP.md` 的 B2 项），
> 该项依赖终端 graphics 协议、工作量大且收益视场景而定，暂缓到独立计划。

## 原则

- **小步可编译**：每个阶段结束都要 `cargo build` 通过、`cargo test` 全绿、
  `cargo clippy --workspace --all-targets -- -D warnings` 无告警、`cargo fmt --all -- --check` 通过（CI 同款，见 `.github/workflows/ci.yml`）。
- **每个可见改动配 PTY 快照测试**：扩展 `crates/atto-ui-chat/src/bin/snapshot_chat_app.rs` + 新增/补充 `crates/atto-ui-chat/tests/pty_chat.rs`（参考 `PtyTestHost`）。
- **模型/store 先行**：需要新数据结构的功能（如消息 fork、权限层级），先扩 `message.rs`/`store.rs`，再挂渲染与交互。
- **运行时同步**：任何模型或输入协议变更，同阶段更新 `src/dynamic.rs` 序列化与 schema，并同步 Node/React 侧类型（`crates/atto-ui-node`、`packages/core`、`packages/react`，见 `docs/NODE_API.md`）。
- **阶段末 review**：每个阶段最后有一个独立的 review 任务，用来复核本阶段改动的正确性与完整性（见 `TODO.md`）。

## 阶段划分

阶段顺序遵循 `AGENT_GAP.md` 的投入产出优先级：先渲染保真（收益最大、改动集中），
再输入补全（交互核心），再会话管理，最后交互增强与细节。

### P1 — 渲染保真度（B1 + B3）
对应 `AGENT_GAP.md` B1、B3。改动集中在 `crates/atto-ui-markdown` 与 chat diff 渲染。

- **代码块语法高亮**：为 markdown crate 的 fenced code block 增加语法高亮。选型（syntect / tree-sitter / 轻量自研 tokenizer）需在阶段初评估并记录到 `AGENT_GAP.md`；优先考虑体积与 `#![forbid(unsafe_code)]` 兼容性。按 fence info string（语言标识）着色，无语言标识时回退纯文本。
- **diff 语法高亮**：在现有 +/- 行着色基础上，对 diff 内容按语言做语法层着色（复用 B1 的高亮引擎），保持 +/- 背景/前景语义不丢失。
- **验收**：`snapshot_markdown_app` / `snapshot_chat_app` 覆盖多语言代码块与带语法高亮的 diff；PTY 快照比对高亮色。

### P2 — 输入补全：斜杠命令 + @文件提及（A1 + A2）
对应 `AGENT_GAP.md` A1、A2。核心是在 `input.rs` 上叠加一个 overlay 补全菜单组件。

- **补全 overlay 组件**：新增一个可复用的 completion popup（列表 + 高亮匹配 + 键盘上下选择 + Enter 确认 + Esc 关闭），锚定在输入框上方/下方。
- **斜杠命令**：输入行首 `/` 触发命令菜单；命令集合可由宿主注入（`register` 回调），内置示例若干（如 `/clear`、`/model`）；选中后写回输入或触发命令回调。
- **@ 文件提及**：输入 `@` 触发文件/资源补全；补全项由宿主提供（文件路径 provider 回调）；确认后在输入中渲染为 mention 芯片或路径文本。
- **运行时同步**：命令/提及协议与回调需在 `dynamic.rs` 暴露，并同步 Node/React 侧。
- **验收**：PTY 覆盖 `/` 触发菜单、过滤、选择、确认；`@` 触发文件补全、确认插入。

### P3 — 会话管理：消息编辑 / 回退 / 重发（C1）
对应 `AGENT_GAP.md` C1。核心是 store 的截断-fork 能力。

- **store 截断-fork API**：新增"截断到某条消息并从该点重新生成"的能力（如 `truncate_from(message_id)` / `fork_at`），保持版本与脏通知约定。
- **编辑 user 消息**：`ChatMessageList` 支持进入某条 user 消息的编辑态，编辑后从该点截断并触发重发回调（`on_edit_and_resubmit`）。
- **retry / regenerate**：对 assistant 回合支持重生成（截断该回合后回调）。与现有 `on_message_action` 的 Retry/Regenerate 打通。
- **验收**：PTY 覆盖编辑 user 消息后截断、retry 后回合截断、fork 后旧消息不再显示。

### P4 — 输入交互增强：排队 & Esc 中断 + 多行编辑（A3 + A4）
对应 `AGENT_GAP.md` A3、A4。

- **输入排队**：流式进行中允许继续输入并"排队"新消息；流式结束后自动出队/提示。
- **Esc 中断语义**：完善 Esc 状态机——一次 Esc 中断当前流式（置 `Canceled`），连按/分级 Esc 的语义明确化，与现有取消按钮统一。
- **多行编辑增强**：多行粘贴规整、（可选）拖入/粘贴文件路径转 `Attachment`。
- **验收**：PTY 覆盖流式中排队新消息、Esc 中断置 `Canceled`、多行粘贴。

### P5 — 会话导航：历史搜索 + Turn 级折叠/引用（C2 + C3）
对应 `AGENT_GAP.md` C2、C3。

- **会话内搜索**：类 Ctrl+R 的搜索/跳转——输入关键词高亮匹配行并可在命中间跳转。
- **Turn 级折叠**：在现有块级折叠之上，支持折叠整个回合（回合 header 上的折叠控件）。
- **引用回复**：（可选）选中某回合/块作为引用附加到下一条输入。
- **验收**：PTY 覆盖搜索命中跳转、turn 折叠/展开、引用附加。

### P6 — 细节层：工具权限层级 + 上下文压缩块（D1 + D2）
对应 `AGENT_GAP.md` D1、D2。

- **工具权限层级**：`ApprovalRequest`/`ApprovalOption` 扩展为支持 allow-once / always / 项目级等层级语义；决策回调携带层级；渲染对应选项。
- **上下文压缩块**：新增专门的 compact 块类型（或扩展 `Notice`），展示压缩进度/前后 token/摘要，区别于普通通知。
- **运行时同步**：模型变更同步 `dynamic.rs` 与 Node/React 侧类型。
- **验收**：PTY 覆盖多层级审批选择与锁定、压缩块渲染。

## 依赖关系

- P1 独立（渲染层），可最先做。
- P2 独立（输入层 overlay）。
- P3 依赖 store，独立于 P1/P2。
- P4 建立在 P2（输入层）之上，且与 P3 的中断/取消语义衔接。
- P5 独立，但 Turn 折叠建立在现有块级折叠之上。
- P6 涉及模型变更，需同步运行时/JS 侧。
- 建议顺序：**P1 → P2 → P3 → P4 → P5 → P6**（即 `AGENT_GAP.md` 优先级顺序）。P1/P2/P3 之间无强依赖，可按资源并行。

## 验证

- 每阶段：`cargo build` / `cargo test`（含 PTY）/ `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check`。
- 关键视觉项用 `snapshot_chat_app` / `snapshot_markdown_app` 抓屏人工比对。
- 涉及 JS 侧的阶段（P2、P6），跑 `npm run smoke --prefix examples/react-tsx` 与 `packages/core` 的 runtime 兼容测试（见 `docs/NODE_API.md`）。
- **每阶段末的 review 任务**必须过：复核本阶段全部改动的正确性与完整性（含边界、错误路径、测试覆盖），并确认全套 CI 命令通过。

## 历史

- Chat 从"通用聊天气泡"重构为"agent 会话视图"阶段的 PLAN/TODO 已归档至 [`docs/archive/2026-07-09-chat-refactor/`](docs/archive/2026-07-09-chat-refactor/)（对应设计文档 `CHAT_UI.md`）。
- 更早的 UI 对齐（Turbo Vision）阶段归档见 [`docs/archive/2026-06-10-ui-gaps/`](docs/archive/2026-06-10-ui-gaps/)。
