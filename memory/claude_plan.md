# 执行计划

本文件记录本次调用的可审计计划、进度和验证结果；不记录私有推理链。

## 范围
- 以 `TODO.md` 为任务顺序和完成状态的唯一权威来源。
- 找到第一个标题未标记 `[DONE]` 的任务并只完成该任务。
- 本轮任务为 `NR18 — 审阅 NT18`，完成后提交并停止。

## 步骤
1. 读取 `TODO.md` 和 `TODO-1.md`，确认 `NR18` 的审阅要求与验收点。
2. 检查最近提交是否直接关联该任务；最近提交 `6e6c117 [NT18] Add React streaming chat example` 纳入审阅范围。
3. 审阅 `examples/node/agent_chat.cjs`、`examples/node/package.json`、`examples/node/README.md` 及其与 `packages/react` / `packages/core` 的运行关系。
4. 如发现阻塞审阅完成的问题，直接修复真实问题；不通过缩小压力用例或放宽语义绕过。
5. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、完整 Rust/TS/JS 验证和示例 smoke/stress。
6. 将 `NR18` 在 `TODO.md` 与 `TODO-1.md` 标记为 `[DONE]`，写入完成记录；仅在阶段计划变化时更新 `PLAN.md`。
7. 检查 `git status`、`git diff` 和最近提交，只暂存本任务相关文件并提交。

## 进度
- 已识别第一个未完成任务：`NR18 — 审阅 NT18`。
- 初始工作区含 `examples/node/agent_chat.cjs`、`examples/node/package.json`、本计划文件改动，以及未跟踪 `notification.sh` / `run_agent.sh`；两个脚本与当前任务无关，保持不动。
- 审阅确认示例覆盖计数器、Button 事件、受控 `TextBox` todo 输入、`ListBox` 选择、多窗口、MenuBar/StatusBar 和 mock/OpenAI/Anthropic 流式聊天路径。
- 修复 stress 路径问题：长 fast mock 流此前会因每 token 提交 state 且每 token 创建 0ms timer 导致 `ATTO_UI_EXAMPLE_STRESS_TOKENS=1500` 超时；现在 fast mock 按批让出事件循环、assistant 文本按固定间隔批量刷新，窗口内渲染有限预览并保留完整回复长度统计。
- 更新 `examples/node/README.md`，补充 clean checkout 依赖安装、headless smoke、stress smoke 和压力参数说明。
- 目标验证已通过：`npm run build:react --prefix examples/node`；`npm run headless --prefix examples/node`；`npm run stress --prefix examples/node`；`ATTO_UI_EXAMPLE_STRESS_TOKENS=1500 npm run stress --prefix examples/node`。
- 完整验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm run typecheck --prefix packages/core`；`npm run typecheck --prefix packages/react`；在 `crates/atto-ui-node` 下运行 `npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`；`npm test --prefix crates/atto-ui-node`；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。
- 已在 `TODO.md` 和 `TODO-1.md` 将 `NR18` 标记为 `[DONE]` 并写入完成记录；`PLAN.md` 未变化，因为阶段级顺序和依赖没有变化。
