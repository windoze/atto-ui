# 执行计划

状态：已在运行仓库命令前写入初始计划。本文件记录可审计计划、决策、进度和验证结果；不记录私有推理链。

## 当前调用计划

1. 先读取 `TODO.md`，选择标题未带 `[DONE]` 的第一个任务。
2. 仅检查最近提交中是否有与该任务直接相关的未完成事项。
3. 阅读所选任务的详情、依赖、验收和邻近完成记录。
4. 只检查完成该任务必需的代码和测试，不做开放式历史问题排查。
5. 按任务要求完成审阅或实现；如需修改，使用小而集中的补丁。
6. 先运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，之后运行任务要求的测试。
7. 若发现未安排的测试或 fixture 失败，先修复，或在 `TODO.md` 中加入最小前置/后续任务后停止。
8. 在 `TODO.md` 与对应任务文件中给完成任务标题加 `[DONE]`，并更新完成记录；仅在阶段计划变化时更新 `PLAN.md`。
9. 复查 `git status`、`git diff` 和最近提交，只提交本任务相关文件。
10. 完成一个任务并提交后停止。

## 进度记录

- 已创建初始计划。下一步读取 `TODO.md` 选择第一个未完成任务。
- 已选择第一个未完成任务：`NR17 — 审阅 NT17`，来源 `TODO-1.md` 阶段 M8。
- 最近提交为 `[NT17] Record completion status`，与当前任务直接相关，但未声明未完成事项；本次将审阅 NT17 e2e 实现并运行所需验证。
- 已审阅 NT17 e2e 文件。e2e app 使用真实 `@atto-ui/react` `render()` 和 native `AppHost`；headless 事件经 `host.sendEvent()` 进入 Rust，tick loop drain native callbacks；PTY 路径在真实 pseudo-terminal 中启动 `node e2e_app.cjs`。
- 针对性 e2e 已通过：`npm run build --prefix packages/react && node packages/react/__test__/e2e.cjs`。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`。
- 当前仓库快照中不存在 `tools/run_fixtures.py`，无单独 fixture 套件可运行。
- 已更新 `TODO.md` 和 `TODO-1.md`，将 `NR17` 标记为 `[DONE]` 并写入完成记录。
- 已提交本任务记录：`68fe09a [NR17] Review React e2e coverage`。当前任务完成，按要求停止。
