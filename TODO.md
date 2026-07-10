# TODO：TUI Agent 对接 DeepSeek / Tool / Skill / Plan Mode

执行计划见 [`PLAN.md`](PLAN.md)，设计文档见 [`TUI_AGENT.md`](TUI_AGENT.md)。

通用验收：每个阶段完成后至少运行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。真实 DeepSeek 测试必须默认 ignored 或手动执行，CI 不依赖外部网络。

## 阶段 M1 - App Skeleton + Mock Provider

- [x] **[DONE] M1.1 新建 `crates/atto-agent-app`** - 创建 crate、加入 workspace、配置依赖 `atto-ui` / `atto-ui-chat` / `atto-ui-async`，保持 `atto-ui` 和 `atto-ui-chat` 不新增网络依赖。
  - 完成记录（2026-07-10）：新增 `crates/atto-agent-app` workspace 成员，添加最小 library/binary 入口和 skeleton 单测；依赖仅配置在 app crate，未修改 `atto-ui` / `atto-ui-chat` 的依赖。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M1.2 组装基础 TUI** - 创建 `Desktop`、状态栏、单窗口 `ChatPanel`，初始化 `ChatMessageStore` 和 `ChatInputHandle`。
  - 完成记录（2026-07-10）：新增 `AgentApp` 构建器，组装 `Desktop`、File/Quit 菜单、自定义状态栏、单个 `ChatPanel` 窗口，并保留 `ChatMessageStore` / `ChatInputHandle` 句柄供后续 turn loop 使用；`run()` 现在启动 crossterm TUI。
  - 验证：`cargo fmt --all`；`cargo clippy --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- [x] **[DONE] M1.3 输入提交闭环** - `on_submit` 写入 user message，启动后台 mock agent turn，通过 `AppAction` 在主线程追加 assistant 流式文本。
  - 完成记录（2026-07-10）：新增 app 私有 `AppAction`，提交文本后追加 user message 和 streaming assistant turn，后台 mock turn 通过 action channel 发送确定性文本 delta/done，主线程 action handler 追加 assistant 文本并完成 turn，同时同步输入 streaming 状态和状态栏 ready/streaming。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`cargo test --workspace --all-targets`。
- [x] **[DONE] M1.4 Slash 命令** - 注入 `/help`、`/clear`、`/plan`、`/skills`、`/tools`、`/abort`，实现基础状态变更和帮助输出。
  - 完成记录（2026-07-10）：app crate 现在注入提交型 slash 命令，并在直接输入 `/cmd` 时走同一命令分派；`/help` 输出命令说明，`/clear` 清空 transcript 并复位 streaming 状态，`/plan` 支持基础 on/off/auto 状态切换并显示在状态栏，`/skills` 和 `/tools` 输出当前 M1 mock 下的空注册表说明，`/abort` 将当前 streaming assistant turn 置为 `Canceled` 并通过 transcript replace 推进 branch token，避免迟到 mock token 污染已取消分支。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`cargo test --workspace --all-targets`。
- [x] **[DONE] M1.5 取消语义** - 接入 `on_cancel` 和 Esc，取消 mock turn，assistant turn 显示 `Canceled`，迟到 token 不污染新分支。
  - 完成记录（2026-07-10）：`ChatMessageStore::cancel_streaming_turn` 现在会在取消 streaming turn 时推进 branch token；agent app 维护当前 mock turn 的取消令牌，并将 `ChatMessageList::on_cancel`、输入 Esc、`/abort` 和 streaming `/clear` 接到同一取消路径，取消后 assistant turn 显示 `Canceled`，迟到 action 因旧 branch token 被拒绝。
  - 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M1.6 快照与 PTY** - 新增 deterministic mock fixture，覆盖输入、流式输出、slash 命令、Esc 取消。
  - 完成记录（2026-07-10）：新增 `snapshot_agent_app` deterministic mock fixture，并让 mock turn delay 可由 fixture 配置以稳定覆盖取消路径；新增 app crate PTY 测试，覆盖普通输入提交、assistant 流式输出、slash 命令输出与状态更新、Esc 取消 active mock turn 且迟到完成文本不出现。
  - 验证：`cargo test -p atto-agent-app --test pty_agent`；`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M1.R Review** - 复核 M1 所有改动，确认 app skeleton 独立、mock 不依赖网络、全套验证通过。
  - 完成记录（2026-07-10）：复核 M1 app crate、deterministic mock fixture、slash/取消路径和 PTY 覆盖；确认 `atto-agent-app` 作为独立 workspace app 组合 `atto-ui` / `atto-ui-chat` / `atto-ui-async`，M1 mock provider 未引入 DeepSeek/API key/网络调用，`atto-ui` 与 `atto-ui-chat` 未新增网络依赖。最近提交未声明与本 review 直接相关的未完成事项。
  - 验证：`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

## 阶段 M2 - DeepSeek Text Streaming

- [x] **[DONE] M2.1 配置加载** - 支持 CLI/env/TOML，读取 `DEEPSEEK_API_KEY`、base URL、model、temperature、max tokens、workspace、plan mode。
  - 完成记录（2026-07-10）：新增 `atto-agent-app::config`，按默认值、用户级 `~/.config/atto-agent/config.toml`、工作区 `.atto-agent.toml`、环境变量、CLI 参数的优先级合并配置；支持 `DEEPSEEK_API_KEY`、`DEEPSEEK_BASE_URL`、`DEEPSEEK_MODEL`、`DEEPSEEK_TEMPERATURE`、`DEEPSEEK_MAX_TOKENS`、`ATTO_AGENT_WORKSPACE`、`ATTO_AGENT_PLAN_MODE`，以及 `--api-key`/`--deepseek-api-key`、`--base-url`、`--model`、`--temperature`、`--max-tokens`、`--workspace`、`--plan-mode`、`--config`、兼容 `--mock`。运行入口加载配置，snapshot fixture 不读取用户环境；状态栏显示配置 model，初始 plan mode 来自配置默认 `auto`。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M2.2 DeepSeek 请求模型** - 定义 request/response/SSE 数据结构，构造 OpenAI-compatible `/chat/completions` 请求。
  - 完成记录（2026-07-10）：新增 `atto_agent_app::deepseek` 协议模块，定义 OpenAI-compatible chat completions request、message、tool schema/tool choice、non-stream response、SSE chunk/delta、tool call delta、usage、finish reason 和 API error 数据结构；新增 `/chat/completions` endpoint 拼接与基于 `AgentConfig` 的 streaming request 构造，保持网络 client/SSE parser/UI 映射留给后续 M2 任务。
  - 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M2.3 SSE parser** - 解析 `data:` 行、`[DONE]`、`choices[].delta.content`、`reasoning_content`、finish_reason 和错误片段。
  - 完成记录（2026-07-10）：在 `deepseek.rs` 新增状态化 `ChatCompletionSseParser`、完整缓冲解析入口和单个 `data:` payload 解析入口；支持分片输入、空行事件分隔、CRLF、注释/非 data 字段忽略、多行 `data:` 聚合、`[DONE]` sentinel、stream chunk JSON、DeepSeek error JSON，并为 malformed JSON 返回带原始片段摘要的错误上下文。
  - 验证：`cargo test -p atto-agent-app deepseek`；`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M2.4 流式 UI 映射** - content 追加到 `TextBlock`，reasoning_content 追加到 `ThinkingBlock`，结束时设置 turn status 和 meta。
  - 完成记录（2026-07-10）：新增 DeepSeek stream UI mapper，将 `choices[].delta.content` 转成 text delta action 写入 `TextBlock`，将 `reasoning_content` 转成 reasoning delta action 并在 assistant turn 中惰性插入默认折叠的 `ThinkingBlock`；`[DONE]` 完成时写入 `ChatMessageMeta`（model、usage、stop_reason）并将 turn 置为 `Complete`。现有 mock turn 也通过 DeepSeek-style content/done event 进入同一映射路径，避免只在测试中覆盖。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M2.5 错误映射** - 401/403、429、5xx、网络断流、JSON 错误映射为 `ChatErrorKind`，UI 显示明确 detail。
  - 完成记录（2026-07-10）：新增 DeepSeek HTTP/API/network/断流/JSON 错误到 `ChatError` 的结构化映射；SSE error event 现在通过 `TurnFailed` action 将 assistant turn 标为 `Failed(ChatError)`，UI header 显示 kind/message/detail；失败 streaming turn 会推进 branch token，避免迟到 token 污染已失败回合。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M2.6 Mock + ignored real 测试** - 单测覆盖 SSE parser；PTY 走 mock client；真实 DeepSeek smoke 标记 ignored。
  - 完成记录（2026-07-10）：新增 app crate 私有 DeepSeek HTTP streaming client，默认单测通过本地 mock HTTP SSE server 覆盖请求构造、Bearer auth、SSE 事件收集和 HTTP 错误映射；保留并验证 PTY snapshot fixture 默认走 mock provider；新增 `deepseek_real_smoke` ignored 真实 DeepSeek streaming smoke，默认测试只编译不访问外网，手动设置 `DEEPSEEK_API_KEY` 后可运行。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-agent-app --all-targets`；`cargo fmt --all -- --check`；`cargo test --workspace --all-targets`。
- [x] **[DONE] M2.R Review** - 复核网络依赖只在 app crate，默认测试无外网，取消和错误路径稳定。
  - 完成记录（2026-07-10）：复核 M2 DeepSeek client、SSE parser、UI mapper、错误映射、取消路径和测试边界；确认 `reqwest` / `futures-util` / app 测试 `tokio` 仅新增于 `crates/atto-agent-app`，`atto-ui` 和 `atto-ui-chat` 未新增网络依赖；默认 DeepSeek client 测试使用本地 mock SSE server，真实 DeepSeek smoke 测试已标记 ignored，需要手动提供 `DEEPSEEK_API_KEY`；取消和失败路径都会推进 branch token，单测和 PTY 覆盖迟到 token 不污染已取消或失败 turn。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。

## 阶段 M3 - Tool Loop + Approval

- [x] **[DONE] M3.1 Tool 抽象** - 定义 `ToolSpec`、`ToolExecutor`、`ToolRegistry`、权限策略、OpenAI tools schema 转换。
  - 完成记录（2026-07-10）：新增 app crate `tool` 模块，定义 `ToolSpec`、`ToolExecutor`、`ToolRegistry`、`ToolContext`、`ToolResult`、`ToolOutputKind`、`ToolPermission`、`ToolPermissionPolicy` 和 `ToolPermissionDecision`；注册表按工具名确定性排序，支持重复注册拒绝、未知工具错误、执行分派，并能将本地工具规格转换为 OpenAI-compatible function tools schema。`/tools` 输出更新为反映抽象层已存在但内置工具尚未注册。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M3.2 Tool call 聚合** - 按 SSE `tool_calls[].index` 聚合 name/arguments，finish_reason 为 `tool_calls` 时进入工具执行阶段。
  - 完成记录（2026-07-10）：DeepSeek UI stream mapper 现在按 `(choice.index, tool_calls[].index)` 确定性聚合 tool call delta，拼接 streamed function name 和 arguments；`finish_reason = tool_calls` 时解析 arguments JSON，生成 `ToolUseBlock { status: Pending }` 并通过主线程 action 插入当前 assistant turn，同时在 `[DONE]` 后以 `StopReason::ToolUse` 完成该 turn。非法或不完整 tool call 会映射为 `ChatErrorKind::Tool`，避免以无效参数进入后续工具阶段。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M3.3 只读工具** - 实现 `read_file`、`list_files`、`search_text`，路径必须限制在 workspace 内。
  - 完成记录（2026-07-10）：新增 app crate 内置只读工具注册入口，`read_file` 支持 workspace 内 UTF-8 文件读取并限制 256 KiB，`list_files` 使用受控 workspace 遍历和 glob 匹配返回相对路径，`search_text` 使用 Rust 实现搜索 UTF-8 文件并返回匹配行摘要；所有工具参数均做 JSON 类型/未知字段校验，输入路径和 symlink 解析后必须仍位于 workspace 内；`/tools` 现在展示 3 个已注册只读工具。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M3.4 副作用工具** - 实现 `apply_patch`、`run_command`，默认需要审批；命令使用 argv，不做 shell 字符串拼接。
  - 完成记录（2026-07-10）：新增内置副作用工具注册入口，完整内置工具表现在包含 `read_file`、`list_files`、`search_text`、`apply_patch`、`run_command`；`apply_patch` 在执行前解析 patch 路径并拒绝绝对路径、`..`、workspace/symlink 逃逸、二进制 patch 和非 UTF-8 既有文件，再通过 `git apply --check` / `git apply` 的 argv 调用从 stdin 应用 patch；`run_command` 仅接受 `argv: string[]` 和 workspace 内 `cwd`，通过 `std::process::Command` 执行，不经过 shell 字符串拼接。两个副作用工具默认 `ApproveForProject`，因此首次执行会要求审批。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M3.5 Approval UI** - 渲染 `ToolUseBlock.approval`，处理 allow once / allow project / deny，deny 时写入失败 tool result。
  - 完成记录（2026-07-10）：agent app 现在在 `ToolCallsReady` 入库前依据内置 `ToolRegistry` 与进程内 `ToolPermissionPolicy` 为需审批工具补充 `ApprovalRequest`；`ChatMessageList::on_approve` 接入 allow once / allow project / deny，项目级允许会记录进程内授权并让后续同工具调用跳过审批，deny 会将 tool use 置为 `Canceled` 并写入失败 `ToolResultBlock`。`AlwaysAllow` 工具直接进入 `Running`，未注册或策略拒绝的工具会生成失败结果。
  - 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`cargo test --workspace --all-targets`。
- [x] **[DONE] M3.6 Tool result 回灌** - `ToolResultBlock` 写 UI，并把 tool result 转成下一次 DeepSeek request 的 role=`tool` 消息。
  - 完成记录（2026-07-10）：agent app 新增后台工具执行回写路径，`AlwaysAllow` 或审批 allow 后的 tool call 会通过 action 在主线程将 `ToolUseBlock` 状态更新为 `Done`/`Error` 并 upsert 对应 `ToolResultBlock`；新增最小 transcript 到 DeepSeek request 的转换入口，assistant `ToolUseBlock` 转 OpenAI-compatible `tool_calls`，`ToolResultBlock` 转 role=`tool` 且携带 `tool_call_id` 的消息，并附带当前内置 tool schema 和 `tool_choice=auto`。补充单测覆盖实际 `read_file` 工具执行写 UI，以及下一次 DeepSeek request 中的 `role=tool` 回灌消息。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M3.7 限制与超时** - 每 turn 限制模型请求数、tool call 数和单工具超时，避免无限循环。
  - 完成记录（2026-07-10）：新增 turn budget tracker，默认限制每个 user turn 最多 8 次模型请求、16 次 tool call、单工具 30 秒超时；提交 turn 时登记首个模型请求，`ToolCallsReady` 按 assistant turn 扣减 tool call 预算并在超限时失败当前 turn，完成、失败、取消和清空会释放预算。工具执行现在带 timeout 上下文，app 层对任意工具等待超时后写入失败 `ToolResultBlock`，`run_command` 和 `apply_patch` 内部子进程也按 timeout 终止，避免长时间卡住。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M3.8 快照与测试** - PTY 覆盖 allow、deny、tool result；单测覆盖非法参数、路径越界、工具不存在。
  - 完成记录（2026-07-10）：扩展 `snapshot_agent_app` deterministic mock fixture，固定 snapshot workspace 到 app crate 根目录，并为 `agent-pty-read-file` / `agent-pty-run-command` 注入 DeepSeek-style tool call 事件；新增 app PTY 覆盖自动 `read_file` tool result、`run_command` allow once 后执行并渲染结果、deny 后写入失败 tool result；新增 app 层未注册工具单测，结合既有只读/副作用工具单测覆盖非法参数和 workspace 越界路径。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app unknown_tool_call_writes_failed_tool_result_without_execution`；`cargo test -p atto-agent-app --test pty_agent`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M3.R Review** - 复核工具权限、安全边界、tool loop 终止条件和测试覆盖。
  - 完成记录（2026-07-10）：复核 M3 工具抽象、内置只读/副作用工具、审批 UI 接入、tool result 回灌、turn budget/timeout 和 PTY 覆盖；确认只读工具做 workspace/symlink 边界校验，`apply_patch` 在执行前校验 patch 路径、文本性和 workspace 归属，`run_command` 使用 argv 并限制 cwd 在 workspace 内，副作用工具默认需要审批；tool call 聚合、未知工具、拒绝、allow once/allow project、tool result 写回、模型/tool call 限制和工具超时均有单测或 PTY 覆盖。最近提交未声明与本 review 直接相关的未完成事项。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。

## 阶段 M4 - Skill Registry

- [x] **[DONE] M4.1 Skill 文件格式** - 解析 `SKILL.md` frontmatter 和 body，支持 name、description、triggers、tools、mode。
  - 完成记录（2026-07-10）：新增 `atto_agent_app::skill` 模块，解析 `SKILL.md` YAML frontmatter 和 Markdown body，支持并校验 `name`、`description`、`triggers`、`tools`、`mode`；`mode` 支持 `manual` / `auto` 且默认 `manual`，`triggers` / `tools` 默认空列表，body 保留原始 Markdown 内容并拒绝空 body。新增 `parse_skill_markdown` / `parse_skill_file` API 和单测覆盖有效文件、默认字段、元数据裁剪、缺失/非法字段、未知 mode、重复列表项、无 frontmatter、空 body 与磁盘文件读取。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app --lib skill`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M4.2 Skill 搜索路径** - 扫描 `.atto/skills` 和 `~/.config/atto-agent/skills`，处理重复 name 和无效文件。
  - 完成记录（2026-07-10）：新增 `SkillRegistry` discovery，按 workspace `.atto/skills` 优先、用户级 `~/.config/atto-agent/skills` 其次递归扫描 `SKILL.md`；缺失目录忽略，非目录/遍历错误/无效 skill 文件记录为非致命 discovery issue，重复 name 保留先发现 skill 并记录冲突。`AgentConfig` 现在保留 `home_dir` 供真实运行扫描用户路径，`AgentConfig::defaults`/snapshot 仍不读取用户 HOME；agent runtime 持有发现到的 registry，`/skills` 输出发现数量和 discovery issue，`/skill` 激活留给 M4.3。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M4.3 手动加载命令** - `/skills` 展示可用 skill，`/skill <name>` 激活 skill，并在状态栏显示数量。
  - 完成记录（2026-07-10）：新增运行时 `LoadedSkillSet` 保存手动激活的 skill；agent app 注入 `/skill` slash 命令，`/skill <name>` 会校验 discovery registry、处理未知/重复加载并更新状态栏 `skills: N`，`/skills` 现在展示 discovered/loaded 数量并标记每个 skill 的 loaded/available 状态。补充单元测试覆盖激活、重复、未知参数和列表标记，更新 PTY slash 测试覆盖状态栏 skill 计数与新 `/skills` 文案。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M4.4 自动加载** - 对用户 prompt 与 name/description/triggers 做确定性词匹配，限制最多加载数量。
  - 完成记录（2026-07-10）：新增 auto-mode skill 确定性词匹配，用户 prompt 与 skill 的 `name`、`description`、`triggers` 做大小写不敏感 token 交集匹配；普通用户提交时自动加载匹配的 `mode: auto` skill，跳过已加载 skill 和 `mode: manual` skill，并按 registry 名称顺序限制每个 prompt 最多自动加载 4 个，同时同步状态栏 `skills: N`。新增单测覆盖字段匹配、大小写/标点 token 化、手动 skill 不自动加载、已加载跳过、上限限制，以及提交路径自动更新 loaded skill 状态。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M4.5 Prompt 注入** - 将已加载 skill 以 `<skills>` 块注入 system prompt，控制单 skill 和总 prompt 大小。
  - 完成记录（2026-07-10）：新增 skill prompt 注入构建器，按设计将已加载 skill 渲染为 `<skills><skill name="..." source="...">...</skill></skills>` system prompt 块；默认限制单个 skill body 最多 6 KiB、完整 skill prompt 最多 20 KiB，并按 UTF-8 边界安全截断。新增带 skill 注入的 DeepSeek transcript request/messages 构建入口，保持无 skill 请求构建入口可用于既有场景；补充单测覆盖 prompt 格式、未加载 skill 忽略、单 skill 与总 prompt 大小限制、UTF-8 截断和 request 注入位置。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app skill_prompt`；`cargo test -p atto-agent-app deepseek_request_from_transcript_injects_loaded_skills`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- [x] **[DONE] M4.6 权限隔离** - skill 只能声明工具偏好，不授予额外工具权限；`run_command` 等仍走审批。
  - 完成记录（2026-07-10）：loaded skill 的 `tools` frontmatter 现在作为 `<skill ... tools="...">` 元数据注入 system prompt，仅表达模型可见工具偏好；DeepSeek request 仍使用 `ToolRegistry` 中的完整注册工具 schema，权限判断仍只由 `ToolSpec.permission` 和 `ToolPermissionPolicy` 决定。新增回归测试覆盖声明 `run_command` 偏好的 skill 不会授予项目级权限，`run_command` tool call 仍渲染 approval 并保持 pending。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-agent-app tool_preferences`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M4.7 测试** - 单测解析、匹配、大小限制、冲突优先级；PTY 覆盖 `/skills` 和 `/skill`。
  - 完成记录（2026-07-10）：复核并保留既有解析、自动匹配和 prompt 大小限制单测，新增同一 skill 搜索根内重复 name 按排序路径确定性保留首个条目的冲突优先级单测；新增 snapshot app 专用 `pty-fixture` skill fixture，并扩展 PTY 覆盖 `/skills` 展示 discovered/available/loaded 状态和 `/skill pty-fixture` 激活后状态栏 `skills: 1`。验证过程中发现带参数 slash 文本在无补全匹配时 Enter 被弹层吞掉，已在 `atto-ui-chat` 修复为回退普通文本提交，并新增输入层回归单测。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app discovery_uses_sorted_paths_for_duplicate_names_within_one_root`；`cargo test -p atto-ui-chat enter_submits_slash_text_with_arguments_when_popup_has_no_match`；`cargo test -p atto-agent-app --test pty_agent`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M4.R Review** - 复核 skill 注入不会泄漏权限、不会破坏 prompt 预算，验证通过。
  - 完成记录（2026-07-10）：复核 M4 skill parser/discovery、手动与自动加载、prompt 注入、工具偏好和权限隔离实现；确认 skill `tools` 仅作为 `<skill ... tools="...">` prompt 元数据注入，不改变 `ToolRegistry` 中的工具 schema 或 `ToolPermissionPolicy`，`run_command` / `apply_patch` 仍默认要求审批。复核 prompt 预算时确认单 skill body 和总 `<skills>` block 均按字节预算截断且保持 UTF-8 边界；同时修复 `LoadedSkillSet` 原先按名称排序导致加载优先级丢失的问题，改为去重但保留加载顺序，并新增 prompt 注入顺序回归测试。最近提交未声明与本 review 直接相关的未完成事项。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app skill_prompt_block_preserves_loaded_order_for_priority`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。

## 阶段 M5 - Plan Mode

- [x] **[DONE] M5.1 Plan mode 状态** - 实现 `off`、`on`、`auto` 配置和 `/plan` 切换，状态栏显示当前模式。
  - 完成记录（2026-07-10）：复核现有 app crate 实现，确认 `PlanMode` 已支持 `off` / `on` / `auto`，配置加载覆盖默认值、用户/工作区 TOML、环境变量与 CLI 参数，运行时从配置初始化状态栏 `plan: <mode>`，`/plan [on|off|auto]` 支持显式设置且无参数按 `off -> on -> auto -> off` 循环切换。
  - 验证：`cargo test -p atto-agent-app plan_mode`；`cargo test -p atto-agent-app config::tests`。本次未修改编译代码；完整 workspace fmt/clippy/test 复用上一条 M4.R 完成记录中的绿色结果。
- [x] **[DONE] M5.2 Auto 判定** - 根据 prompt 和工具需求粗判是否涉及写文件、命令、代码修改等副作用。
  - 完成记录（2026-07-10）：新增 app crate `plan` 模块，提供 deterministic `PlanTurnDecision`，按 `PlanMode::off/on/auto`、用户 prompt、注册工具权限判定当前 turn 是否需要先进入 plan mode；auto 模式会识别 mutating tool 需求（如 `apply_patch` / `run_command`）、命令执行意图、代码/文件修改意图，并保留纯问答和只读工具检查为 direct。用户提交路径现在计算该判定并随 turn request 传递，供后续 M5.3 计划生成消费，同时不提前改变 mock streaming 行为。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M5.3 计划生成** - 实现虚拟 tool `submit_plan({ items })`，兜底解析 markdown 列表为 `PlanItem`。
  - 完成记录（2026-07-10）：新增 plan-mode 虚拟 `submit_plan` tool schema、forced tool choice 和 plan draft system prompt；`DeepSeekUiStream` 现在可在需要计划时把 `submit_plan({ items })` 参数直接映射为 pending `PlanBlock`，并在模型未调用虚拟 tool 时解析 markdown 有序/无序/checklist 列表作为 fallback。`submit_plan` 未注册进本地 `ToolRegistry`，不会作为本地工具执行或授予额外权限；mock plan turn 现在通过 DeepSeek-style `tool_calls` 走同一映射路径。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M5.4 PlanBlock UI** - 渲染 `PlanBlock { decision: Pending }`，接入 `on_plan_decision`。
  - 完成记录（2026-07-10）：复核 `atto-ui-chat` 已有 `PlanDecisionView` 可渲染 pending plan、展示 Accept/Reject 操作并发出 `PlanDecisionEvent`；agent app 现在在构建 `ChatMessageList` 时绑定 `on_plan_decision`，通过 `handle_plan_decision` 将 pending `PlanBlock` 锁定为 accepted/rejected，并忽略对已决 plan 的旧事件覆盖。新增 app 层回归测试覆盖 plan 决策状态更新。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M5.5 Accept/Reject 流程** - Accept 后追加内部执行指令并继续 agent loop；Reject 后停止当前 turn。
  - 完成记录（2026-07-10）：`on_plan_decision` 现在接入 plan decision runtime；Accept 会锁定 `PlanBlock` 为 accepted，结束计划等待 turn，追加内部 system 指令 `The user accepted the plan. Execute the accepted plan now. Use tools only when needed and obey approval policy.`，并以 direct 模式启动后续 assistant 执行 turn；Reject 会锁定为 rejected，取消/释放计划 turn、恢复输入和状态栏，不追加内部指令也不启动执行。补充 app 层单测覆盖决策锁定、Accept 继续执行和 Reject 停止。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app plan_decision`；`cargo test -p atto-agent-app accepting_plan_appends_internal_instruction_and_starts_execution_turn`；`cargo test -p atto-agent-app rejecting_plan_stops_turn_without_starting_execution`；`cargo test -p atto-agent-app --all-targets`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M5.6 副作用工具门控** - 计划接受前拦截 `apply_patch`、`run_command` 等 mutating tool，并写入明确 tool result。
  - 完成记录（2026-07-10）：新增 per-turn mutating-tool gate，`plan: on` / `plan: auto` 下未接受计划前会在 app 层优先拦截 `apply_patch`、`run_command` 等非 `AlwaysAllow` 工具，写入失败 `ToolResultBlock`：`Plan mode blocks mutating tools until the plan is accepted.`；项目级 allow grant 不能绕过该 gate，接受计划后的执行 turn 和 `plan: off` 仍走既有审批/执行路径。plan stream 中模型误发非 `submit_plan` tool call 时现在交由同一 gate 处理，旧 PTY approval 用例已显式切换 `/plan off` 以继续验证审批 UI。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-agent-app plan_gate_blocks_mutating_tool_even_with_project_grant`；`cargo test -p atto-agent-app deepseek_stream_plan_turn_mutating_tool_call_writes_blocked_tool_result`；`cargo test -p atto-agent-app --test pty_agent`；`cargo fmt --all -- --check`；`cargo test --workspace --all-targets`。
- [x] **[DONE] M5.7 快照与测试** - PTY 覆盖计划生成、Accept 后执行、Reject 后停止、未接受计划时工具被拒绝。
  - 完成记录（2026-07-10）：扩展 `snapshot_agent_app` PTY 覆盖 plan mode：auto plan prompt 会渲染 pending `PlanBlock` 和 Accept/Reject 操作；点击 Accept 后锁定为 accepted 并启动后续 mock execution turn；点击 Reject 后锁定为 rejected 且不追加执行 turn；默认 `plan: auto` 下 `run_command` tool call 会在计划接受前被拦截并写入失败 tool result，不渲染审批 UI 或成功结果。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app --test pty_agent`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M5.R Review** - 复核 plan mode 不依赖模型特殊能力，副作用门控不可绕过，验证通过。
  - 完成记录（2026-07-10）：复核 M5 plan mode 的本地 deterministic 判定、虚拟 `submit_plan` tool、markdown fallback、PlanBlock Accept/Reject 续跑/停止流程，以及副作用工具 gate 与权限策略的执行顺序；确认 plan 草稿请求只暴露虚拟 `submit_plan`，不依赖模型特有能力，且 `apply_patch` / `run_command` 等 mutating tool 在计划接受前会由 app 层 gate 优先拦截，项目级授权不能绕过。最近提交未声明与本 review 直接相关的未完成事项。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。

## 阶段 M6 - Context / Session Polish

- [x] **[DONE] M6.1 ContextBuilder** - 将 UI transcript 转成 DeepSeek messages，正确处理 user、assistant、tool use、tool result、notice、compact、skills。
  - 完成记录（2026-07-10）：新增 `atto_agent_app::context::ContextBuilder`，集中负责 UI transcript 到 DeepSeek/OpenAI-compatible messages 的转换；现有 `deepseek_*_from_transcript*` request/message API 保持不变并委托 ContextBuilder。转换现在按 block 顺序处理 user/system/custom/assistant 文本，assistant `ToolUseBlock` 聚合为 `tool_calls`，`ToolResultBlock` 映射为 role=`tool`，`NoticeBlock` 和 `CompactBlock` 映射为独立 system context 消息，已加载 skills 仍作为 `<skills>` system prompt 注入到 transcript 前。新增单测覆盖 user/assistant/tool/notice/compact 转换和 loaded skill 注入。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M6.2 文件 mention** - 解析 `@path`，读取 workspace 内文件摘要注入 prompt，限制单文件和总大小。
  - 完成记录（2026-07-10）：`ContextBuilder` 现在可解析用户消息中的 `@path` mention，按 workspace canonicalize 后读取 UTF-8 文件摘要，并以 `<context_files>` 追加到 user prompt；文件读取限制为单文件最多 32 KiB、单条用户消息总计最多 128 KiB，越界、缺失、目录和非 UTF-8 文件会作为明确错误条目注入而不会泄漏文件内容。普通 DeepSeek request 与 plan request 构建入口现在均使用 `AgentConfig.workspace` 启用 file mention expansion。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-agent-app context_builder`；`cargo test -p atto-agent-app deepseek_request_from_transcript_injects_file_mentions_from_config_workspace`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M6.3 工具输出预算** - 回传模型的 tool output 做截断，UI 保留完整或尾部窗口。
  - 完成记录（2026-07-10）：`ContextBuilder` 现在对每条 role=`tool` 消息应用默认 16 KiB 模型上下文预算，超长 `ToolResultBlock` 输出会按 UTF-8 边界截断并追加明确的模型可见截断说明；截断只发生在 DeepSeek request/message 构建边界，UI transcript 中的 `ToolResultBlock` 保持完整，既有 ANSI 工具输出尾部窗口能力继续负责 UI 长输出展示。新增单测覆盖长工具输出截断、UI 输出不被修改和多字节字符边界安全。
  - 验证：`cargo test -p atto-agent-app context_builder`；`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M6.4 Compact** - 超预算时生成 `CompactBlock`，后续请求使用摘要替代旧 turn。
  - 完成记录（2026-07-10）：新增 agent app deterministic compact 策略，默认按 64K 上下文窗口 70% 阈值触发、保留最近 20 条消息，并用本地摘要兜底生成完成态 `CompactBlock { before_tokens, after_tokens, summary }`；提交用户消息和接受 plan 后继续执行前都会在 assistant turn 启动前压缩已 settled 的早期 transcript，跳过 streaming、pending approval/plan/diff/task 等仍需交互的块。压缩通过 `ChatMessageStore::replace_all` 推进 branch token，后续 DeepSeek request 由既有 `ContextBuilder` 将 `<compact>` system context 发送给模型，而不再重放被压缩旧 turn 的完整消息。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-agent-app compact`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M6.5 Retry/Edit 重跑** - 接入 `on_edit_and_resubmit`、retry/regenerate，截断后重启 agent turn。
  - 完成记录（2026-07-10）：agent app 现在注册 `ChatMessageList::on_edit_and_resubmit` 和 retry/regenerate `on_message_action`；编辑重发会在 chat 控件截断旧 user 分支后追加编辑后的 user message，并复用普通提交路径重新计算 skill 自动加载、plan 判定、compact 和 mock turn 启动；retry/regenerate 会使用截断后 transcript 中最后一个 user prompt 重启 assistant turn。分支变更后会取消当前后台 mock turn、释放 turn budget，并依赖 branch token 拒绝旧 turn 的迟到 token。补充 app 层单测覆盖编辑重发、retry/regenerate 重启和旧 branch 迟到 token 拒绝；同时调整 plan accept PTY 断言，避免启用 Retry/Regenerate 按钮后因内容高度增加导致 accepted 标记滚出视口而误报失败。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app --lib edit_and_resubmit_appends_edited_user_and_restarts_turn`；`cargo test -p atto-agent-app --lib retry_and_regenerate_restart_from_retained_user_prompt_and_reject_late_tokens`；`cargo test -p atto-agent-app --test pty_agent agent_plan_mode_generates_plan_and_accept_continues_execution`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M6.6 Transcript 持久化（可选）** - 支持 JSONL 保存和恢复，默认可关闭。
  - 完成记录（2026-07-10）：新增 app crate 私有 `transcript` JSONL 持久化模块，每行保存一个带 schema version 的 `ChatMessage`，显式覆盖当前 UI transcript 的消息、block、meta、审批、错误和嵌套 task transcript；新增默认关闭的配置项 `transcript_path`，支持 TOML、`ATTO_AGENT_TRANSCRIPT` 和 `--transcript`，相对路径按 workspace 解析。真实运行路径现在会在启动时按配置恢复 transcript，运行中用 dirty observer 保存变更，退出后做最终保存；恢复时未完成 streaming turn 会标为 `Canceled`，避免重启后显示不可恢复的活动流。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app transcript`；`cargo test -p atto-agent-app config::tests`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`cargo test --workspace --all-targets`。
- [x] **[DONE] M6.7 状态栏完善** - 显示 model、plan、tools、skills、streaming、token 估算和错误摘要。
  - 完成记录（2026-07-10）：agent app 状态栏现在包含 model、plan、tools、skills、streaming、`tokens~N` transcript token 估算和 `err:*` 最新错误摘要；新增 `TranscriptStatusState` 聚合 token/error 状态，并在用户提交、slash 命令、取消、plan 决策、tool 审批和后台 action 变更后同步状态。复用 compact transcript token 估算逻辑，错误摘要覆盖 failed turn 和失败 tool result，并修复 tool-call budget 失败分支早退导致状态不同步的问题。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-agent-app --lib tool_call_budget_fails_turn_before_appending_over_limit`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M6.8 快照与测试** - PTY 覆盖 mention、compact、retry/edit、取消后无迟到 token。
  - 完成记录（2026-07-10）：扩展 deterministic `snapshot_agent_app` fixture，新增 snapshot 专用小 compact 阈值和 context probe mock prompt，context probe 通过 `deepseek_request_from_transcript` 走真实 file mention 展开路径；新增 PTY 覆盖 `@.atto/skills/pty-fixture/SKILL.md` mention 注入、长会话触发 `CompactBlock` 并滚动查看摘要、Retry 重启 assistant turn、Edit 修改用户 prompt 后重跑，以及保留既有 Esc 取消后迟到 token 不显示的 PTY 覆盖。Retry/Edit PTY 使用可见递增 mock turn 序号验证按钮实际触发重跑。
  - 验证：`cargo fmt --all`；`cargo test -p atto-agent-app --test pty_agent`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。
- [x] **[DONE] M6.R Review** - 复核上下文预算、分支 token、长会话性能和全套验证。
  - 完成记录（2026-07-10）：复核 M6 ContextBuilder、file mention、tool output budget、compact、retry/edit、transcript 持久化、状态栏和 PTY 覆盖；确认分支 token 取消/重跑路径已有单测和 PTY 覆盖，迟到 token 不污染新分支。复核中发现 file mention 展开未纳入 compact/status 预算、mention 注入总预算未包含错误/wrapper 开销、非 UTF-8 截断文件可能泄漏前缀，以及启用 transcript 持久化时流式 delta 可能频繁同步全量保存；已修复为 file mention 硬总预算与数量上限、严格 UTF-8 校验、compact/status 保守计入 mention 展开预算、compact 长输出限量摘录、transcript 脏保存节流并保留退出强制落盘。最近提交未声明与本 review 直接相关的未完成事项。
  - 验证：`cargo fmt --all`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`；`cargo fmt --all -- --check`。

## 收尾

- [ ] **Docs 更新** - 根据实际实现更新 `TUI_AGENT.md`、README 或新增 app README。
- [ ] **CI 检查** - 确认默认 CI 不依赖 `DEEPSEEK_API_KEY` 或网络。
- [ ] **Release 检查** - 如新增 crate 需要发布策略，补充 `docs/RELEASE.md` 或说明其仅作为 workspace app。
