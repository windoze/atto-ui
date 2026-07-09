# TUI_AGENT.md - 简单 TUI Agent 设计

本文设计一个基于现有 chat 组件的最小可落地 TUI agent。目标不是继续扩展 `atto-ui-chat` 的控件能力，而是在其上新增一个应用层 agent：对接 DeepSeek Chat Completions API，支持本地 tool、skill 注入和 plan mode。

## 目标

| 目标 | 说明 |
|---|---|
| 单窗口 TUI agent | 主界面使用 `ChatPanel`，消息流由 `ChatMessageStore` 驱动。 |
| DeepSeek 流式对话 | 使用 OpenAI-compatible `/chat/completions`，支持 SSE token 流和 tool calls。 |
| tool 调用 | 模型可请求本地工具；工具执行经过权限层级审批，结果回灌模型。 |
| skill 注入 | skill 是可复用的指令包，可手动选择，也可按提示词简单匹配自动加载。 |
| plan mode | 执行前先产出计划，渲染为 `PlanBlock`，用户接受后才允许写入或命令类工具。 |
| 保持核心干净 | `reqwest`、`tokio`、DeepSeek 协议只放在新 app crate，不进入 `atto-ui` 或 `atto-ui-chat`。 |

## 非目标

| 非目标 | 原因 |
|---|---|
| 多 agent 调度 | MVP 只做单 agent loop；`TaskBlock` 可作为后续子任务 UI。 |
| MCP 兼容层 | 先定义本地 `ToolRegistry`，以后可加 MCP adapter。 |
| 图片/多模态 | 现有计划明确暂不做内联图片渲染。 |
| 长期记忆系统 | MVP 只保留当前会话和可选 transcript 文件。 |
| 完整 shell 沙箱 | MVP 做 workspace 路径约束和审批，不承诺强隔离。 |

## 代码位置

新增独立应用 crate：`crates/atto-agent-app`。

| 路径 | 职责 |
|---|---|
| `crates/atto-agent-app/src/main.rs` | CLI 参数、配置加载、启动 TUI。 |
| `src/app.rs` | 组装 `Desktop`、菜单、状态栏、`ChatPanel`。 |
| `src/agent.rs` | agent turn 状态机、模型循环、tool 循环、plan gate。 |
| `src/deepseek.rs` | DeepSeek 请求、SSE 解析、错误映射。 |
| `src/tool.rs` | `ToolRegistry`、工具 schema、审批策略、执行器。 |
| `src/skill.rs` | skill 文件解析、索引、选择、prompt 注入。 |
| `src/context.rs` | 会话到 DeepSeek messages 的转换、token 预算、压缩。 |
| `src/config.rs` | 环境变量和 TOML 配置。 |
| `src/action.rs` | 后台任务到 UI 主线程的 `AppAction` 枚举。 |
| `src/prompts.rs` | system prompt、plan mode prompt、tool 使用约束。 |

建议依赖只加到 `crates/atto-agent-app/Cargo.toml`：

```toml
[dependencies]
anyhow = "1"
atto-ui = { path = "../.." }
atto-ui-chat = { path = "../atto-ui-chat" }
atto-ui-async = { path = "../atto-ui-async", features = ["event-stream", "tokio-runtime"] }
futures-util = "0.3"
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
toml = "0.8"
```

`atto-ui` 和 `atto-ui-chat` 不新增网络或 tokio 依赖。

## 运行拓扑

```text
Terminal
  |
  v
atto-ui-async run loop
  |
  +-- ChatPanel / ChatMessageStore / ChatInputHandle
  |
  +-- AppAction channel
        |
        v
      Agent runtime task
        |
        +-- DeepSeekClient
        +-- ToolRegistry
        +-- SkillRegistry
        +-- ContextBuilder
```

UI 状态只能在主线程通过 `AppAction` 更新。后台任务只发送动作，不直接改 `ChatMessageStore`。

## UI 设计

主窗口使用 `ChatPanel::new(list, input)`。

| UI 区域 | 内容 |
|---|---|
| MenuBar | `Session`、`Model`、`Tools`、`Plan`。MVP 可为空菜单，仅保留状态栏。 |
| StatusBar left | `model=deepseek-chat plan=off tools=ready skills=0`。 |
| StatusBar right | `Esc cancel · Ctrl+Q quit · /help`。 |
| MessageList | 展示 user、assistant、thinking、tool、plan、compact、error。 |
| InputPanel | 多行输入，slash 命令，`@` 文件提及，流式时排队。 |

消息块映射：

| Agent 事件 | `atto-ui-chat` 映射 |
|---|---|
| 用户输入 | `ChatMessage { role: User, blocks: [Text] }`。 |
| assistant 普通 token | `TextBlock`，使用 `append_text_delta` 流式追加。 |
| DeepSeek `reasoning_content` | `ThinkingBlock`，默认折叠，可流式追加。 |
| 模型请求 tool | `ToolUseBlock`，带 `ApprovalRequest` 或直接运行。 |
| 工具输出 | `ToolResultBlock`，`Ansi`、`Markdown` 或 `Diff`。 |
| 计划 | `PlanBlock`，`Pending` 时显示 Accept/Reject。 |
| 上下文压缩 | `CompactBlock`。 |
| API 或工具错误 | `ChatTurnStatus::Failed(ChatError)` 或 `NoticeBlock::Error`。 |

`ChatMessageList` 需要挂接这些回调：

| 回调 | 用途 |
|---|---|
| `on_approve` | 用户选择 allow once / always / project / deny 后继续或取消 tool。 |
| `on_plan_decision` | plan accepted 后进入执行阶段，rejected 后回到输入。 |
| `on_cancel` | Esc 或取消按钮取消当前 agent turn。 |
| `on_edit_and_resubmit` | 编辑 user 消息后截断并重跑。 |
| `on_message_action` | retry / regenerate 截断后重跑。 |
| `on_edit_decision` | 后续如果工具产出 diff，可支持 Accept/Reject。 |

## Slash 命令

内置命令通过 `ChatInputHandle::set_slash_commands` 注入。

| 命令 | 行为 |
|---|---|
| `/help` | 插入或提交帮助提示。 |
| `/clear` | 清空当前会话，保留配置。 |
| `/model` | 展示或切换模型，例如 `deepseek-chat`。 |
| `/plan` | 切换 plan mode：`/plan on`、`/plan off`、`/plan auto`。 |
| `/skills` | 列出可用 skill。 |
| `/skill <name>` | 手动附加 skill 到下一次或当前会话。 |
| `/tools` | 列出工具和审批策略。 |
| `/compact` | 触发上下文压缩。 |
| `/abort` | 取消当前流式任务。 |

`@` mention provider 先复用仓库文件候选。候选确认后以 `@path/to/file` 写入输入，`ContextBuilder` 在发起请求前解析这些 mention 并添加只读文件摘要。

## 配置

配置来源按优先级覆盖：CLI 参数、环境变量、工作区 `.atto-agent.toml`、用户级 `~/.config/atto-agent/config.toml`、默认值。

```toml
model = "deepseek-chat"
base_url = "https://api.deepseek.com/v1"
temperature = 0.2
max_tokens = 4096
plan_mode = "auto"
workspace = "."

[tools]
read_file = "allow"
list_files = "allow"
search_text = "allow"
apply_patch = "approve"
run_command = "approve"

[skills]
paths = [".atto/skills", "~/.config/atto-agent/skills"]
auto_load = true
max_loaded = 4
```

环境变量：

| 变量 | 说明 |
|---|---|
| `DEEPSEEK_API_KEY` | 必填，DeepSeek API key。 |
| `DEEPSEEK_BASE_URL` | 可选，默认 `https://api.deepseek.com/v1`。 |
| `DEEPSEEK_MODEL` | 可选，默认 `deepseek-chat`。 |
| `ATTO_AGENT_PLAN_MODE` | `off`、`on`、`auto`。 |
| `ATTO_AGENT_WORKSPACE` | workspace root，默认当前目录。 |

## DeepSeek 对接

DeepSeek 走 OpenAI-compatible Chat Completions。

| 项 | 设计 |
|---|---|
| Endpoint | `POST {base_url}/chat/completions`。 |
| Auth | `Authorization: Bearer ${DEEPSEEK_API_KEY}`。 |
| 默认模型 | `deepseek-chat`，因为 MVP 需要 tool calling。 |
| 可选模型 | `deepseek-reasoner` 可用于只读规划或解释，但不假设其 tool calling 能力。 |
| Streaming | `stream: true`，解析 SSE `data:` 行直到 `[DONE]`。 |
| Tools | 使用 OpenAI function calling 格式生成 `tools`。 |
| Tool choice | 正常模式 `auto`；plan draft 阶段为 `none` 或只允许虚拟 `submit_plan`。 |

请求结构示意：

```json
{
  "model": "deepseek-chat",
  "messages": [
    { "role": "system", "content": "...agent policy..." },
    { "role": "user", "content": "..." }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "read_file",
        "description": "Read a UTF-8 file under the workspace root.",
        "parameters": {
          "type": "object",
          "properties": { "path": { "type": "string" } },
          "required": ["path"]
        }
      }
    }
  ],
  "tool_choice": "auto",
  "temperature": 0.2,
  "max_tokens": 4096,
  "stream": true
}
```

SSE 解析规则：

| 字段 | 处理 |
|---|---|
| `choices[].delta.content` | 追加到当前 `TextBlock`。 |
| `choices[].delta.reasoning_content` | 追加到当前 `ThinkingBlock`。 |
| `choices[].delta.tool_calls[].index` | 以 index 聚合同一个 tool call 的增量。 |
| `tool_calls[].function.name` | 设置 tool 名称。 |
| `tool_calls[].function.arguments` | 字符串增量拼接，结束后按 JSON 解析。 |
| `finish_reason == "tool_calls"` | 执行已聚合 tool calls，然后将结果作为 role=`tool` 消息继续请求。 |
| `finish_reason == "stop"` | 结束 turn，写入 `ChatTurnStatus::Complete`。 |

错误映射：

| 错误 | UI 映射 |
|---|---|
| 401 / 403 | `ChatErrorKind::Api`，提示检查 `DEEPSEEK_API_KEY`。 |
| 429 | `ChatErrorKind::RateLimit`。 |
| 5xx | `ChatErrorKind::Api`，detail 保留响应体摘要。 |
| 网络超时 / 断流 | `ChatErrorKind::Network`。 |
| JSON 解析失败 | `ChatErrorKind::Api`，detail 保留原始片段摘要。 |
| 工具参数非法 | `ChatErrorKind::Tool` 或 tool result `ok=false`。 |

## Agent 状态机

```text
Idle
  -> UserSubmitted
  -> Planning?        plan mode on/auto 且任务可能有副作用
  -> WaitingPlan      渲染 PlanBlock(Pending)
  -> RunningModel     DeepSeek stream
  -> WaitingApproval  工具需要用户审批
  -> RunningTool      执行本地工具
  -> RunningModel     带 tool result 继续请求
  -> Complete
```

核心流程：

1. 输入提交后主线程 push user message。
2. 捕获 `ChatMessageStore::branch_token()`，后台 agent task 只在 token 仍当前时发送 UI action。
3. `ContextBuilder` 生成 system prompt、历史 messages、已加载 skills、可用 tools。
4. plan mode 需要计划时，先请求计划并渲染 `PlanBlock(Pending)`。
5. 用户接受计划后，追加一条内部 system 事件：`User accepted the plan. Execute it now.`。
6. 模型流式输出时按 chunk 更新 `ThinkingBlock` 和 `TextBlock`。
7. 模型产生 tool calls 时，创建 `ToolUseBlock`，根据策略决定是否审批。
8. 工具执行结束后 upsert `ToolResultBlock`，并把 tool result 追加进下一次 DeepSeek request。
9. 没有 tool calls 且 finish_reason 为 stop 时，turn 完成。
10. Esc 或取消按钮触发 `CancellationToken`，网络流和工具执行协作退出，turn 置为 `Canceled`。

为了避免无限循环，MVP 设置：每个 user turn 最多 8 次模型请求、最多 16 次 tool call、单个工具默认 30 秒超时。

## Tool 系统

工具接口：

```rust
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub permission: ToolPermission,
    pub output: ToolOutputKind,
}

pub enum ToolPermission {
    AlwaysAllow,
    ApproveOnce,
    ApproveForProject,
    NeverAllow,
}

pub trait ToolExecutor: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn execute(&self, ctx: ToolContext, args: serde_json::Value) -> anyhow::Result<ToolResult>;
}
```

MVP 内置工具：

| 工具 | 权限 | 说明 |
|---|---|---|
| `read_file` | allow | 读取 workspace 内 UTF-8 文本文件，大小默认限制 256 KiB。 |
| `list_files` | allow | 基于 glob 列文件，隐藏目录和 ignore 规则后续补。 |
| `search_text` | allow | 调用 `rg` 或 Rust 实现搜索，返回匹配行摘要。 |
| `apply_patch` | approve | 只允许修改 workspace 内文本文件，显示 diff 后执行。 |
| `run_command` | approve | 默认需要审批，输出以 ANSI tool result 流式显示。 |

审批策略：

| 场景 | UI |
|---|---|
| `AlwaysAllow` | 直接运行，仍渲染 `ToolUseBlock`。 |
| `ApproveOnce` | 显示 allow once / deny。 |
| `ApproveForProject` | 显示 allow once / allow for project / deny。 |
| `NeverAllow` | 不执行，生成 `ToolResultBlock { ok=false }`。 |
| 用户 deny | `ToolUseBlock` 置 `Canceled`，tool result 写入 `User denied tool call`。 |

安全边界：

| 边界 | 要求 |
|---|---|
| 路径 | 所有文件路径 canonicalize 后必须位于 workspace root 内。 |
| symlink | symlink 解析后的真实路径仍必须在 workspace 内。 |
| 命令 | `run_command` 不通过 shell 拼接字符串；使用 argv 数组。 |
| 密钥 | 不把 `DEEPSEEK_API_KEY` 注入 prompt、工具输出或 transcript。 |
| 持久权限 | MVP 的 allow for project 只在进程内有效；落盘需单独设计。 |

## Skill 系统

skill 是本地指令包，不是可执行插件。它的主要作用是给模型注入领域约束、工作流、示例和默认工具偏好。

目录约定：

```text
.atto/skills/<skill-id>/SKILL.md
~/.config/atto-agent/skills/<skill-id>/SKILL.md
```

`SKILL.md` 格式：

```markdown
---
name: rust-review
description: Review Rust code for correctness, safety, tests, and API regressions.
triggers: ["review", "rust", "clippy"]
tools: ["read_file", "search_text"]
mode: auto
---

When this skill is active, inspect changed Rust files first, then report findings with file and line references.
Prefer tests that reproduce behavioral regressions.
```

加载规则：

| 规则 | 说明 |
|---|---|
| 手动加载 | `/skill rust-review` 将 skill 附加到当前会话或下一 turn。 |
| 自动加载 | 对用户 prompt 与 `name`、`description`、`triggers` 做简单词匹配。 |
| 数量限制 | 默认最多加载 4 个 skill。 |
| 大小限制 | 单个 skill body 默认最多 6 KiB，总 skill prompt 默认最多 20 KiB。 |
| 冲突处理 | 多个 skill 指令冲突时，后加载的手动 skill 优先于自动 skill。 |

注入方式：

```text
<skills>
<skill name="rust-review" source=".atto/skills/rust-review/SKILL.md">
...skill body...
</skill>
</skills>
```

Skill 只声明工具偏好，不直接获得额外权限。即使 skill 要求 `run_command`，仍必须通过 `ToolRegistry` 的审批策略。

## Plan Mode

Plan mode 是 app 层执行门控，不依赖 DeepSeek 的特殊模式。

| 模式 | 行为 |
|---|---|
| `off` | 直接执行，模型可按工具策略请求 tool。 |
| `on` | 每个 user turn 先生成计划，计划被接受前禁止副作用工具。 |
| `auto` | 读写文件、运行命令、修改代码等任务自动进入 plan mode；纯问答不进入。 |

Plan 阶段策略：

| 阶段 | 工具权限 |
|---|---|
| 计划前调查 | 只允许 `read_file`、`list_files`、`search_text` 等只读工具。 |
| 计划生成 | 请求模型输出 3 到 7 条具体步骤。 |
| 等待用户 | 渲染 `PlanBlock { decision: Pending }`。 |
| 接受计划 | `PlanDecision::Accepted`，开启执行阶段。 |
| 拒绝计划 | `PlanDecision::Rejected`，本 turn 停止，用户可补充要求。 |

计划生成有两个实现选项：

| 选项 | 说明 |
|---|---|
| 优先方案 | 暴露虚拟 tool `submit_plan({ items: string[] })`，`tool_choice` 指定它；该 tool 不执行外部副作用，只把参数转成 `PlanBlock`。 |
| 兜底方案 | 若模型没有调用 `submit_plan`，解析 assistant markdown 中的有序列表或 checklist。 |

Plan prompt 核心约束：

```text
You are in plan mode. Do not modify files, run commands with side effects, or call mutating tools.
Produce a concise execution plan. Each item must be actionable and verifiable.
Wait for user approval before execution.
```

执行阶段会追加内部消息：

```text
The user accepted the plan. Execute the accepted plan now. Use tools only when needed and obey approval policy.
```

如果模型在计划接受前请求 `apply_patch` 或 `run_command`，agent 不执行该 tool，并追加 tool result：`Plan mode blocks mutating tools until the plan is accepted.`。

## Context Builder

DeepSeek messages 由当前 UI transcript 转换而来。

| 来源 | 转换 |
|---|---|
| `ChatRole::User` | OpenAI role `user`。 |
| `ChatRole::Assistant` text/thinking | role `assistant`，thinking 可作为内部摘要，不强制回传完整推理。 |
| `ToolUseBlock` | assistant `tool_calls`。 |
| `ToolResultBlock` | role `tool`，带 `tool_call_id`。 |
| `NoticeBlock` / `CompactBlock` | system 或 developer-style context 文本。 |
| skills | system prompt 中 `<skills>` 块。 |
| mentions | user prompt 后附 `<context_files>` 摘要。 |

上下文预算：

| 项 | 默认 |
|---|---|
| 最近 turn | 保留完整最近 20 条消息。 |
| 工具输出 | 每个 tool result 回传模型最多 16 KiB，UI 可保留更多。 |
| skill prompt | 总计最多 20 KiB。 |
| 文件 mention | 单文件最多 32 KiB，总计最多 128 KiB。 |
| 压缩阈值 | 估算超过模型上下文 70% 时触发 compact。 |

压缩流程：

1. 选取较早的 user/assistant/tool turn。
2. 调用 DeepSeek 生成摘要，或使用本地截断摘要兜底。
3. 插入 `CompactBlock { status: Complete, before_tokens, after_tokens, summary }`。
4. 后续 request 用摘要替代被压缩 turn 的完整内容。

## AppAction

后台任务通过 channel 给主线程发送动作。

```rust
pub enum AppAction {
    UserSubmitted(String),
    StartTurn { prompt: String, mode: PlanMode },
    AssistantStarted { message_id: ChatMessageId, text_block_id: ChatBlockId },
    AppendText { block_id: ChatBlockId, delta: String },
    AppendThinking { block_id: ChatBlockId, delta: String },
    ToolRequested { message_id: ChatMessageId, block: ToolUseBlock },
    ToolOutput { call_id: String, result: ToolResultBlock },
    ApprovalResolved { block_id: ChatBlockId, option_id: String },
    PlanReady { message_id: ChatMessageId, block: PlanBlock },
    PlanResolved { block_id: ChatBlockId, decision: PlanDecision },
    TurnComplete { message_id: ChatMessageId, meta: ChatMessageMeta },
    TurnFailed { message_id: ChatMessageId, error: ChatError },
    TurnCanceled { message_id: ChatMessageId },
    SetStatus(String),
}
```

实际实现时可以合并部分 action，但原则是：后台任务不持有 UI 组件，也不直接写 reactive state。

## 最小执行伪代码

```rust
async fn run_agent_turn(ctx: AgentTurnContext) -> anyhow::Result<()> {
    let branch = ctx.store.branch_token();

    if ctx.plan_mode.requires_plan(&ctx.user_prompt) {
        let plan = request_plan(&ctx).await?;
        ctx.actions.send(AppAction::PlanReady { message_id: ctx.assistant_id, block: plan })?;
        return Ok(());
    }

    let mut messages = ctx.context_builder.build_messages()?;
    let mut requests = 0usize;

    loop {
        requests += 1;
        if requests > ctx.limits.max_model_requests {
            anyhow::bail!("model request limit reached");
        }
        if !ctx.store.is_branch_current(branch) || ctx.cancel.is_cancelled() {
            ctx.actions.send(AppAction::TurnCanceled { message_id: ctx.assistant_id }).ok();
            return Ok(());
        }

        let response = ctx.deepseek.stream_chat(messages.clone(), ctx.tool_registry.deepseek_tools()).await?;
        let tool_calls = stream_response_to_ui(response, &ctx).await?;

        if tool_calls.is_empty() {
            ctx.actions.send(AppAction::TurnComplete { message_id: ctx.assistant_id, meta: ctx.meta() })?;
            return Ok(());
        }

        let tool_results = run_tools_with_approval(tool_calls, &ctx).await?;
        messages.extend(tool_results.into_deepseek_messages());
    }
}
```

## 测试计划

| 层级 | 测试 |
|---|---|
| Unit | DeepSeek SSE parser：content、reasoning_content、tool_calls 增量、`[DONE]`、错误 JSON。 |
| Unit | Tool schema 转 OpenAI tools，参数校验，路径越界拒绝。 |
| Unit | Skill frontmatter 解析、自动匹配、大小限制、冲突优先级。 |
| Unit | Plan parser：虚拟 `submit_plan`、markdown fallback、拒绝 mutating tool。 |
| PTY | mock DeepSeek 流式文本渲染到 `TextBlock`。 |
| PTY | tool approval：allow once 后执行，deny 后取消。 |
| PTY | plan mode：生成计划、Accept 后执行、Reject 后停止。 |
| PTY | `/skill` 和 `/plan` slash 命令可见并生效。 |
| PTY | Esc 取消网络流或长工具，turn 显示 canceled。 |
| Ignored integration | 设置 `DEEPSEEK_API_KEY` 后请求真实 DeepSeek，默认不在 CI 跑。 |

测试用 DeepSeek client 需要 trait 化：

```rust
#[async_trait]
pub trait ChatClient {
    async fn stream_chat(&self, request: ChatRequest) -> anyhow::Result<ChatStream>;
}
```

MVP 可避免 `async-trait`，改用 enum 或 boxed future；若引入 `async-trait`，只加在 app crate。

## 分阶段落地

| 阶段 | 产出 | 验收 |
|---|---|---|
| M1 App skeleton | `crates/atto-agent-app`、ChatPanel、slash 命令、mock provider。 | `cargo run -p atto-agent-app -- --mock` 可本地对话。 |
| M2 DeepSeek text stream | `DeepSeekClient`、SSE parser、文本和 thinking 流式 UI。 | mock PTY + ignored real API smoke。 |
| M3 Tool loop | ToolRegistry、内置 read/search/apply_patch/run_command、approval UI。 | PTY 覆盖 allow/deny/tool result。 |
| M4 Skill registry | skill 文件解析、`/skills`、`/skill`、自动匹配、prompt 注入。 | Unit + PTY 覆盖 skill 加载。 |
| M5 Plan mode | plan 状态机、`PlanBlock`、Accept/Reject、mutating tool gate。 | PTY 覆盖 plan accept/reject。 |
| M6 Context polish | mention 文件上下文、compact、retry/edit 重跑、错误处理。 | PTY 覆盖取消、retry、compact。 |

每阶段至少运行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

## 关键取舍

| 取舍 | 结论 |
|---|---|
| Rust app 还是 Node app | 选择 Rust app，直接复用 `atto-ui-chat` 的完整块模型和 `atto-ui-async`。 |
| 直接在 chat crate 加 agent | 不加。chat crate 保持 UI 控件，agent 是应用层。 |
| plan mode 是否依赖模型特殊能力 | 不依赖。用 app 层 gate 和 prompt 约束实现。 |
| skill 是否能执行代码 | 不能。skill 只是 prompt 包和工具偏好，权限仍由 tool 层控制。 |
| 默认是否允许写文件 | 不允许。写文件和命令都需要审批，plan mode 接受前仍会被拦截。 |

## 后续可扩展点

| 扩展 | 说明 |
|---|---|
| MCP adapter | 把 MCP server 映射为 `ToolExecutor`。 |
| 子 agent | 使用 `TaskBlock` 展示子任务 transcript。 |
| 持久会话 | transcript JSONL 和会话恢复。 |
| 项目级权限落盘 | 明确 schema 后保存 allow-project 决策。 |
| 更强 skill 选择 | 后续可加 embedding 或 LLM router，MVP 先用确定性匹配。 |
| diff apply 体验 | 将 `apply_patch` 的 diff 同步渲染为 `DiffBlock`，用户 Accept 后落盘。 |
