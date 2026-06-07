# 执行计划

状态：已在运行仓库命令前写入本次调用初始计划。本文件记录可审计计划、决策、进度和验证结果；不记录私有推理链。

## 当前任务

- 已从 `TODO.md` / `TODO-1.md` 识别第一个未完成任务：`NT18 — 示例 app（含流式聊天）（T.3）`。
- 任务要求：在 `examples/` 下提供 JS/TS 示例，覆盖计数器、待办表单、流式聊天；手动运行并记录结果，确认 state、事件、受控输入和流式高频更新可用。

## 执行步骤

1. 检查最近提交是否明确提到与 `NT18` 直接相关的未完成事项；如有，纳入当前任务或在 `TODO.md` 中加入前置任务。
2. 查看现有 `examples/`、`packages/core`、`packages/react` 与测试入口，确定示例应复用的构建/运行方式。
3. 设计最小但完整的示例形态：计数器、受控待办输入/列表、模拟 token 流式聊天；避免真实外部 SDK 密钥依赖，必要时用可替换的 async token source 展示接入点。
4. 实现示例文件与必要的 package script / README 说明；优先复用现有 React 包构建产物与 native loader，不引入不必要依赖。
5. 如流式高频更新暴露性能或事件循环问题，先修正真实问题；只有在确有必要时加入明确的限频机制，并在示例中说明原因。
6. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，之后运行 Rust/TS/JS 相关测试与示例手动验证。
7. 若发现未安排的测试/fixture 失败，修复或把最小前置/后续任务写入 `TODO.md` 后停止；不把当前任务标为完成。
8. 完成后更新 `TODO.md` 和 `TODO-1.md`：将 `NT18` 标题标记 `[DONE]`，写入完成记录与验证命令；仅在阶段计划变化时更新 `PLAN.md`。
9. 检查 `git status`、`git diff`、最近提交，确认只包含本任务相关变更，然后提交，提交信息使用 `[NT18] ...` 格式。
10. 提交后停止，不进入 `NR18`。

## 进度记录

- 已读取任务索引与 `TODO-1.md` 详情，确认当前任务为 `NT18`。
- 初始计划已写入；下一步检查最近提交和示例/包结构。
- 最近提交未声明与 `NT18` 相关的未完成事项；已有未跟踪 `notification.sh` / `run_agent.sh` 与本任务无关，将保持不动。
- 已新增 `examples/node/agent_chat.cjs`、`README.md`、`package.json`：默认离线 mock token 流，支持可选 OpenAI/Anthropic SDK provider，并提供 headless smoke 模式。
- Headless smoke 已扩展为发送 native key events，覆盖计数器点击、受控 todo 输入和添加 todo；快速运行 `ATTO_UI_EXAMPLE_HEADLESS=1 node examples/node/agent_chat.cjs --fast` 已通过并输出快照摘要。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm run typecheck --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm test --prefix crates/atto-ui-node`；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`npm run headless --prefix examples/node`；`git diff --check`。未找到 `tools/run_fixtures.py`。
- 交互式 PTY smoke 已通过：真实终端启动 `node examples/node/agent_chat.cjs --fast`，等待 `Streaming Chat` 与 `Assistant:` 渲染后发送 `Ctrl+Q`，进程干净退出。
- 已更新 `TODO.md` 与 `TODO-1.md`，将 `NT18` 标记为 `[DONE]` 并写入完成记录；`PLAN.md` 未变化。
