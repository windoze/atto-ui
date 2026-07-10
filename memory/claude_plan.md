# 执行计划

## 约束

- 以 `TODO.md` 为当前任务的唯一权威来源。
- 每次只完成第一个未标记 `[DONE]` 的任务，完成后提交并停止。
- 不记录私有推理链；本文件记录可审计计划、关键决策、执行进度和验证结果。
- 任何测试失败若未被明确排期，必须修复或在 `TODO.md` 中新增最小必要前置任务。

## 初始步骤

1. 读取 `TODO.md`，识别第一个标题未带 `[DONE]` 的任务。
2. 检查最新提交信息是否明确提到与该任务直接相关的未完成事项。
3. 读取当前任务涉及的代码、测试和文档，确认需求、依赖和验证要求。
4. 按任务要求做最小正确实现；若发现阻塞性前置问题，则更新 `TODO.md` 并停止。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试套件。
6. 更新 `TODO.md`：将完成任务标题加 `[DONE]`，填写完成记录和验证结果。
7. 视需要仅在阶段级计划变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交，提交本次任务相关改动。

## 当前状态

- 状态：已读取 M3.7 相关设计和现有 app/tool 代码，准备实现限制与超时。
- 最新提交：`4319bc3 [M3.6] Record execution plan completion`，未声明与 M3.7 直接相关的未完成阻塞项。

## M3.7 执行计划

1. 阅读 `PLAN.md` / `TUI_AGENT.md` 中 M3 工具循环、限制和超时相关要求。
2. 定位 `atto-agent-app` 中 agent turn、DeepSeek 请求、tool call 聚合、工具审批和工具执行代码。
3. 确定现有 turn loop 是否已经支持多轮模型请求；若尚未形成完整循环，仍在当前边界内加入可验证的请求数、tool call 数和单工具超时限制，避免未来循环无限执行。
4. 实现限制配置或常量、运行时计数、错误/状态映射，以及工具超时执行路径。
5. 添加覆盖限制和超时的最小测试，优先使用单测；只在需要验证 UI 行为时补充 PTY。
6. 运行格式化、lint 和完整测试。
7. 更新 `TODO.md` 中 M3.7 标题为 `[DONE]` 并填写完成记录。
8. 提交本次任务所有相关改动后停止。

## M3.7 设计记录

- `TUI_AGENT.md` 指定默认限制：每个 user turn 最多 8 次模型请求、16 次 tool call、单工具 30 秒超时。
- 当前 app 仍以 mock turn 驱动 UI，真实 DeepSeek 多轮请求循环尚未接入；因此本次在现有提交和 `ToolCallsReady` / 工具执行边界加入预算跟踪，后续真实循环可复用同一 tracker 扣减模型请求预算。
- 工具执行将使用 `ToolContext` 中的 timeout；app 层对任意工具线程做等待超时，`run_command` / `git apply` 这类子进程工具还会在工具层按 timeout 杀掉超时进程。

## M3.7 进度

- 已新增 `limits` 模块，提供默认 turn 限制和 `TurnBudgetTracker`。
- 已将预算接入提交、取消、清空、`ToolCallsReady`、turn 完成/失败清理路径。
- 已将工具执行改为带超时等待，并在 mutating 子进程工具中加入超时 kill；随后补强 stdout/stderr pipe 读取，避免超时后被继承 pipe 拖住。
- 已添加单测覆盖模型请求预算、tool call 超限失败和单工具超时失败 result。
- 已运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`，全部通过。
- 已更新 `TODO.md`，将 M3.7 标记为 `[DONE]` 并填写完成记录。
- 下一步：检查 git 状态/diff/最近提交，然后提交本次任务改动并停止。
