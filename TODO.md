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

- [ ] **M2.1 配置加载** - 支持 CLI/env/TOML，读取 `DEEPSEEK_API_KEY`、base URL、model、temperature、max tokens、workspace、plan mode。
- [ ] **M2.2 DeepSeek 请求模型** - 定义 request/response/SSE 数据结构，构造 OpenAI-compatible `/chat/completions` 请求。
- [ ] **M2.3 SSE parser** - 解析 `data:` 行、`[DONE]`、`choices[].delta.content`、`reasoning_content`、finish_reason 和错误片段。
- [ ] **M2.4 流式 UI 映射** - content 追加到 `TextBlock`，reasoning_content 追加到 `ThinkingBlock`，结束时设置 turn status 和 meta。
- [ ] **M2.5 错误映射** - 401/403、429、5xx、网络断流、JSON 错误映射为 `ChatErrorKind`，UI 显示明确 detail。
- [ ] **M2.6 Mock + ignored real 测试** - 单测覆盖 SSE parser；PTY 走 mock client；真实 DeepSeek smoke 标记 ignored。
- [ ] **M2.R Review** - 复核网络依赖只在 app crate，默认测试无外网，取消和错误路径稳定。

## 阶段 M3 - Tool Loop + Approval

- [ ] **M3.1 Tool 抽象** - 定义 `ToolSpec`、`ToolExecutor`、`ToolRegistry`、权限策略、OpenAI tools schema 转换。
- [ ] **M3.2 Tool call 聚合** - 按 SSE `tool_calls[].index` 聚合 name/arguments，finish_reason 为 `tool_calls` 时进入工具执行阶段。
- [ ] **M3.3 只读工具** - 实现 `read_file`、`list_files`、`search_text`，路径必须限制在 workspace 内。
- [ ] **M3.4 副作用工具** - 实现 `apply_patch`、`run_command`，默认需要审批；命令使用 argv，不做 shell 字符串拼接。
- [ ] **M3.5 Approval UI** - 渲染 `ToolUseBlock.approval`，处理 allow once / allow project / deny，deny 时写入失败 tool result。
- [ ] **M3.6 Tool result 回灌** - `ToolResultBlock` 写 UI，并把 tool result 转成下一次 DeepSeek request 的 role=`tool` 消息。
- [ ] **M3.7 限制与超时** - 每 turn 限制模型请求数、tool call 数和单工具超时，避免无限循环。
- [ ] **M3.8 快照与测试** - PTY 覆盖 allow、deny、tool result；单测覆盖非法参数、路径越界、工具不存在。
- [ ] **M3.R Review** - 复核工具权限、安全边界、tool loop 终止条件和测试覆盖。

## 阶段 M4 - Skill Registry

- [ ] **M4.1 Skill 文件格式** - 解析 `SKILL.md` frontmatter 和 body，支持 name、description、triggers、tools、mode。
- [ ] **M4.2 Skill 搜索路径** - 扫描 `.atto/skills` 和 `~/.config/atto-agent/skills`，处理重复 name 和无效文件。
- [ ] **M4.3 手动加载命令** - `/skills` 展示可用 skill，`/skill <name>` 激活 skill，并在状态栏显示数量。
- [ ] **M4.4 自动加载** - 对用户 prompt 与 name/description/triggers 做确定性词匹配，限制最多加载数量。
- [ ] **M4.5 Prompt 注入** - 将已加载 skill 以 `<skills>` 块注入 system prompt，控制单 skill 和总 prompt 大小。
- [ ] **M4.6 权限隔离** - skill 只能声明工具偏好，不授予额外工具权限；`run_command` 等仍走审批。
- [ ] **M4.7 测试** - 单测解析、匹配、大小限制、冲突优先级；PTY 覆盖 `/skills` 和 `/skill`。
- [ ] **M4.R Review** - 复核 skill 注入不会泄漏权限、不会破坏 prompt 预算，验证通过。

## 阶段 M5 - Plan Mode

- [ ] **M5.1 Plan mode 状态** - 实现 `off`、`on`、`auto` 配置和 `/plan` 切换，状态栏显示当前模式。
- [ ] **M5.2 Auto 判定** - 根据 prompt 和工具需求粗判是否涉及写文件、命令、代码修改等副作用。
- [ ] **M5.3 计划生成** - 实现虚拟 tool `submit_plan({ items })`，兜底解析 markdown 列表为 `PlanItem`。
- [ ] **M5.4 PlanBlock UI** - 渲染 `PlanBlock { decision: Pending }`，接入 `on_plan_decision`。
- [ ] **M5.5 Accept/Reject 流程** - Accept 后追加内部执行指令并继续 agent loop；Reject 后停止当前 turn。
- [ ] **M5.6 副作用工具门控** - 计划接受前拦截 `apply_patch`、`run_command` 等 mutating tool，并写入明确 tool result。
- [ ] **M5.7 快照与测试** - PTY 覆盖计划生成、Accept 后执行、Reject 后停止、未接受计划时工具被拒绝。
- [ ] **M5.R Review** - 复核 plan mode 不依赖模型特殊能力，副作用门控不可绕过，验证通过。

## 阶段 M6 - Context / Session Polish

- [ ] **M6.1 ContextBuilder** - 将 UI transcript 转成 DeepSeek messages，正确处理 user、assistant、tool use、tool result、notice、compact、skills。
- [ ] **M6.2 文件 mention** - 解析 `@path`，读取 workspace 内文件摘要注入 prompt，限制单文件和总大小。
- [ ] **M6.3 工具输出预算** - 回传模型的 tool output 做截断，UI 保留完整或尾部窗口。
- [ ] **M6.4 Compact** - 超预算时生成 `CompactBlock`，后续请求使用摘要替代旧 turn。
- [ ] **M6.5 Retry/Edit 重跑** - 接入 `on_edit_and_resubmit`、retry/regenerate，截断后重启 agent turn。
- [ ] **M6.6 Transcript 持久化（可选）** - 支持 JSONL 保存和恢复，默认可关闭。
- [ ] **M6.7 状态栏完善** - 显示 model、plan、tools、skills、streaming、token 估算和错误摘要。
- [ ] **M6.8 快照与测试** - PTY 覆盖 mention、compact、retry/edit、取消后无迟到 token。
- [ ] **M6.R Review** - 复核上下文预算、分支 token、长会话性能和全套验证。

## 收尾

- [ ] **Docs 更新** - 根据实际实现更新 `TUI_AGENT.md`、README 或新增 app README。
- [ ] **CI 检查** - 确认默认 CI 不依赖 `DEEPSEEK_API_KEY` 或网络。
- [ ] **Release 检查** - 如新增 crate 需要发布策略，补充 `docs/RELEASE.md` 或说明其仅作为 workspace app。
