# 执行计划：TUI Agent 对接 DeepSeek / Tool / Skill / Plan Mode

本计划对应 [`TUI_AGENT.md`](TUI_AGENT.md)。目标是在现有 `atto-ui-chat` 能力已经齐备的基础上，新增一个应用层 TUI agent：复用 `ChatPanel` / `ChatMessageStore` / `ChatInputHandle`，对接 DeepSeek API，支持本地 tool、skill 注入和 plan mode。

旧的 chat 控件能力补齐计划已归档至 [`docs/archive/2026-07-10-chat-capabilities/`](docs/archive/2026-07-10-chat-capabilities/)。

## 当前状态（2026-07-11 复核）

M1-M6 已把 DeepSeek 接入所需的所有*零件*实现并单测覆盖，但它们尚未组装成一次真实的 live turn：

- `crates/atto-agent-app/src/deepseek_client.rs` 的 `DeepSeekClient::stream_chat_completions`（真实 HTTP + SSE）目前**只被 `tests/deepseek_real_smoke.rs`（`#[ignore]`）调用**。
- 实际的 `run()` / 提交路径**始终走 `MockTurnRegistry`**（见 `start_mock_agent_turn_for_prompt`），没有任何"检测到 `DEEPSEEK_API_KEY` 就切换到真实 provider"的分支。
- 真实客户端 `collect_sse_events` 会**缓冲整段响应**（收到 `[DONE]` 后一次性返回 `Vec<ChatCompletionSseEvent>`），无法逐 token 流式写入 UI。
- `run()` 内**没有 async turn 驱动**（mock turn 跑在普通线程上）、**没有真实的多轮 tool loop**、**没有进行中 HTTP 请求的取消**。
- 状态栏 `provider` 段写死为 `provider: mock`（`src/lib.rs`）。

因此新增 **M7 - Live DeepSeek 接入**，把已有零件真正接线成可用的 DeepSeek agent，用 `.envrc` 中的 `DEEPSEEK_API_KEY` 做手动/ignored 验证。

## 范围

| 范围 | 说明 |
|---|---|
| 新应用 crate | 新增 `crates/atto-agent-app`，作为 Rust TUI agent 应用。 |
| UI 复用 | 不重做 chat 控件；直接使用 `atto-ui-chat` 的块模型、输入补全、审批、plan、取消、编辑重发等能力。 |
| DeepSeek | 使用 OpenAI-compatible Chat Completions 和 SSE streaming。 |
| Tool | 建立本地 `ToolRegistry`，MVP 包含读文件、列文件、搜索、apply patch、run command。 |
| Skill | 建立本地 `SkillRegistry`，解析 `SKILL.md`，支持手动和简单自动加载。 |
| Plan mode | app 层执行门控：计划未接受前禁止副作用工具。 |
| 真实 provider 接入 | 把已有 `DeepSeekClient` / SSE parser / `DeepSeekUiStream` / `ContextBuilder` / `ToolRegistry` 组装成真实的 live agent turn（M7）。 |

## 非范围

| 非范围 | 说明 |
|---|---|
| 扩展 `atto-ui-chat` 数据模型 | 当前已有 `PlanBlock`、`ToolUseBlock`、`TaskBlock`、审批、compact 等能力，除非发现阻塞 bug，否则不改。 |
| MCP | MVP 先做本地 tool registry，MCP adapter 后续单独计划。 |
| 多 agent 调度 | MVP 只做单 agent loop。 |
| 图片/多模态 | 继续不纳入本计划。 |
| 强沙箱 | MVP 做 workspace 路径约束、审批和命令 argv 化，不承诺系统级隔离。 |

## 原则

| 原则 | 要求 |
|---|---|
| 应用层隔离 | `reqwest`、`tokio`、DeepSeek 协议只进入 `crates/atto-agent-app`。 |
| 核心依赖干净 | `atto-ui` 和 `atto-ui-chat` 不新增网络依赖。 |
| UI 主线程更新 | 后台任务只发 `AppAction`，主线程更新 `ChatMessageStore` 和 bindings。 |
| 可测试优先 | DeepSeek client trait/enum 化，MVP 全部关键路径可用 mock provider 测。 |
| 安全默认 | 写文件、apply patch、run command 默认需要审批；plan mode 接受前始终拦截副作用工具。 |
| 小步可编译 | 每阶段结束必须能 build/test/clippy/fmt。 |

## 阶段划分

### M1 - App Skeleton + Mock Provider

建立 `crates/atto-agent-app`，完成可运行的 TUI shell。

| 产出 | 说明 |
|---|---|
| crate 注册 | workspace 加入 `crates/atto-agent-app`。 |
| UI 组装 | `Desktop` + `MenuBar`/`StatusBar` + 单窗口 `ChatPanel`。 |
| 输入提交 | `ChatInputPanel::on_submit` push user message，并启动 mock agent turn。 |
| Slash 命令 | 注入 `/help`、`/clear`、`/plan`、`/skills`、`/tools`。 |
| Mock stream | 不依赖网络，按 token 流式写入 assistant `TextBlock`。 |
| 取消 | Esc / cancel 回调取消 mock turn 并置 `Canceled`。 |

验收：`cargo run -p atto-agent-app -- --mock` 可本地交互；PTY 覆盖 mock stream、slash command、Esc cancel。

### M2 - DeepSeek Text Streaming

接入 DeepSeek Chat Completions 的基础文本流。

| 产出 | 说明 |
|---|---|
| 配置加载 | CLI/env/TOML：`DEEPSEEK_API_KEY`、base URL、model、temperature、max tokens。 |
| DeepSeekClient | `POST /chat/completions`，`stream: true`。 |
| SSE parser | 解析 `data:`、`[DONE]`、content、reasoning_content、finish_reason。 |
| UI 映射 | content -> `TextBlock`，reasoning_content -> `ThinkingBlock`。 |
| 错误映射 | 401/403、429、5xx、网络、JSON 错误映射到 `ChatError`。 |
| real API smoke | 提供 `#[ignore]` 或手动命令，不进默认 CI。 |

验收：mock PTY 稳定；设置 `DEEPSEEK_API_KEY` 后可手动跑真实 streaming；失败时 UI 显示清晰错误。

### M3 - Tool Loop + Approval

实现 DeepSeek function calling 到本地 tool 的闭环。

| 产出 | 说明 |
|---|---|
| Tool schema | `ToolSpec` 转 OpenAI-compatible `tools`。 |
| Tool call 聚合 | 按 SSE `tool_calls[].index` 聚合 name 和 arguments。 |
| ToolRegistry | 注册、查找、参数校验、权限策略。 |
| 内置只读工具 | `read_file`、`list_files`、`search_text`。 |
| 内置副作用工具 | `apply_patch`、`run_command`，默认审批。 |
| Approval UI | `ToolUseBlock.approval` + `ChatMessageList::on_approve`。 |
| Tool result | `ToolResultBlock` 写 UI，并作为 role=`tool` 继续请求模型。 |
| 限制 | 每 turn 最大模型请求数、tool call 数、工具超时。 |

验收：PTY 覆盖 tool 请求、allow once、deny、tool result 回灌；单测覆盖路径越界、非法参数、无限循环限制。

### M4 - Skill Registry

实现 skill 文件格式、索引、选择和 prompt 注入。

| 产出 | 说明 |
|---|---|
| Skill parser | 解析 `SKILL.md` frontmatter + body。 |
| 搜索路径 | `.atto/skills` 和 `~/.config/atto-agent/skills`。 |
| 手动加载 | `/skills` 列表，`/skill <name>` 激活。 |
| 自动加载 | 按 prompt 与 name/description/triggers 的简单词匹配。 |
| Prompt 注入 | system prompt 增加 `<skills>` 块。 |
| 安全约束 | skill 只影响提示词和工具偏好，不授予额外工具权限。 |
| 预算 | 单 skill、总 skill prompt 大小限制。 |

验收：单测覆盖解析、匹配、大小限制、冲突优先级；PTY 覆盖 `/skills` 和 `/skill` 生效。

### M5 - Plan Mode

实现 app 层计划门控。

| 产出 | 说明 |
|---|---|
| 模式 | `off`、`on`、`auto`，支持 `/plan` 切换。 |
| auto 判定 | 根据用户意图和工具需求粗判是否可能有副作用。 |
| 计划生成 | 优先使用虚拟 tool `submit_plan({ items })`；兜底解析 markdown 列表。 |
| Plan UI | 渲染 `PlanBlock { decision: Pending }`。 |
| 接受 | `PlanDecision::Accepted` 后追加内部执行指令并继续 agent loop。 |
| 拒绝 | `PlanDecision::Rejected` 后停止当前 turn，等待用户补充。 |
| 副作用拦截 | 计划接受前拒绝 `apply_patch`、`run_command` 等 mutating tool。 |

验收：PTY 覆盖 plan 生成、Accept 后执行、Reject 后停止、未接受计划时副作用工具被拦截。

### M6 - Context / Session Polish

补齐上下文、mention、compact、编辑重发和稳定性。

| 产出 | 说明 |
|---|---|
| ContextBuilder | UI transcript -> DeepSeek messages。 |
| 文件 mention | `@path` 转只读文件摘要。 |
| 工具输出预算 | 回传模型的 tool output 截断，UI 保留完整或尾部窗口。 |
| Compact | 超预算时生成 `CompactBlock`，后续请求使用摘要。 |
| Retry/Edit | `on_edit_and_resubmit`、retry/regenerate 触发截断并重跑。 |
| Transcript | 可选 JSONL 保存和恢复。 |
| 状态栏 | 展示 model、plan、tools、skills、streaming、token 估算。 |

验收：PTY 覆盖 mention、compact、retry/edit 重跑；长会话不阻塞 UI；取消后无迟到 token 污染新分支。

### M7 - Live DeepSeek 接入

把 M1-M6 已实现的零件组装成真实可用的 DeepSeek agent turn。这是"真正接入 DeepSeek"的核心里程碑。

| 产出 | 说明 |
|---|---|
| Provider 选择 | 引入 `AgentProvider`（`Mock` / `DeepSeek`）：默认按配置解析——存在有效 `DEEPSEEK_API_KEY` 且未强制 `--mock` 时选 DeepSeek，否则 mock；snapshot fixture 始终 mock。 |
| 流式增量事件 | 让 `DeepSeekClient` 提供逐块回调/channel（不再等 `[DONE]` 一次性返回），把每个 `ChatCompletionSseEvent` 尽快推给主线程。 |
| Async turn 驱动 | 在 app 内建立 tokio runtime（或复用 `atto-ui-async`），后台跑真实 turn，只通过 `AppAction` 更新 UI；真实 turn 复用现有 `DeepSeekUiStream` 映射，与 mock 走同一条 action 路径。 |
| 请求构造 | 提交/继续 turn 时用 `ContextBuilder` 从当前 transcript 构造 messages（含 skills、file mention、compact、tool 回灌），带上注册工具 schema。 |
| 真实 tool loop | `finish_reason = tool_calls` 后执行本地工具、审批门控、写回 `role=tool`，再自动发起下一轮真实请求，直到无 tool call 或触达 turn budget。 |
| 取消 | Esc / `on_cancel` / `/abort` 能中止进行中的 HTTP 请求（abort handle / drop），推进 branch token，迟到事件不污染新分支。 |
| 错误映射 | 真实路径复用 M2.5 的 `ChatError` 映射；缺失/无效 API key、401/403、429、5xx、断流在 UI 显示清晰错误。 |
| 状态栏 | `provider` 段反映实际 provider（`mock` / `deepseek`），不再写死。 |
| 验证 | 默认测试仍走 mock 与本地 mock SSE server；新增 `#[ignore]` 的真实端到端 turn 冒烟（含一次 tool 往返），手动用 `.envrc` 的 `DEEPSEEK_API_KEY` 运行。 |

验收：设置 `DEEPSEEK_API_KEY` 后 `cargo run -p atto-agent-app` 能与真实 DeepSeek 交互（文本流式 + 至少一次 tool 往返 + plan mode 生效）；取消能中止真实请求；默认 CI/测试不触外网；mock PTY 全部保持绿色。

## 依赖关系

| 阶段 | 依赖 |
|---|---|
| M1 | 无，先搭应用骨架和 mock。 |
| M2 | 依赖 M1 的 action loop 和 UI 映射。 |
| M3 | 依赖 M2 的 DeepSeek request/stream 基础。 |
| M4 | 依赖 M1/M2 的 prompt 构建入口，可与 M3 部分并行。 |
| M5 | 依赖 M3 的 tool gate 和 `PlanBlock` 回调。 |
| M6 | 依赖 M2-M5，作为收尾和体验完善。 |
| M7 | 依赖 M2-M6 的全部零件（client/SSE/UI 映射/context/tool loop/plan/skill/compact），把它们组装成真实 live turn。 |

建议顺序：M1 -> M2 -> M3 -> M4 -> M5 -> M6 -> M7。

## 验证

每阶段至少运行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

涉及真实 DeepSeek 的测试默认忽略或手动执行（`.envrc` 已提供 `DEEPSEEK_API_KEY`，`direnv allow` 后即可）：

```sh
# M7 后：默认检测到 API key 即走真实 DeepSeek，可手动交互验证文本流 / tool 往返 / plan mode
cargo run -p atto-agent-app
# 强制 mock，不触外网
cargo run -p atto-agent-app -- --mock
# 仅运行 ignored 的真实端到端冒烟
cargo test -p atto-agent-app -- --ignored
```

PTY 覆盖应优先走 mock client，不依赖网络和外部 API。
