# 执行计划

## 范围

- 以 `TODO.md` 为任务顺序、任务要求、验证要求和完成状态的权威来源。
- 只完成首个标题未标记 `[DONE]` 的任务；完成后停止，不进入下一项。
- `PLAN.md` 仅在阶段级顺序、依赖、假设或完成标准变化时更新。

## 执行步骤

1. 读取 `TODO.md`，确认首个标题未带 `[DONE]` 的任务。
2. 只检查最新提交中与该任务直接相关的未完成事项。
3. 阅读该任务在 `TODO-2.md` 中的审阅要求、依赖、验收点和相关实现文件。
4. 按 `R3` 要求审阅 T3 docking API、reserve/layout、Desktop work area、window state 区分和测试覆盖。
5. 若发现阻塞当前任务的缺陷，直接修复；若是无法在本任务内正确完成的前置问题，则在 `TODO.md` / `TODO-2.md` 插入最小前置任务并停止。
6. 补足审阅任务要求中缺失的最小回归测试。
7. 依次运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
8. 在 `TODO.md` 和 `TODO-2.md` 将 `R3` 标记为 `[DONE]`，并写入完成记录。
9. 查看 git 状态、diff 和最近提交，只提交本任务相关文件。
10. 提交后停止，不处理 `T4`。

## 进度记录

- 已在仓库检查和实现前创建本计划。
- 已从 `TODO.md` 确认首个未完成任务为 `R3` / `审阅 T3`，来源为 `TODO-2.md`。
- 最新提交为 `[T3] Add window docking layout`，与 `R3` 直接相关；审阅范围聚焦 T3 docking public API、reserve/layout 行为、窗口状态差异和测试覆盖。
- 已补充两条审阅回归测试：dock layout 限定在 Desktop `work_area` 内；maximized modal 保持在 dock-reserved work area 内，同时 active modal 阻止 dock hit-test。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- 已在 `TODO.md` 和 `TODO-2.md` 将 `R3` 标记为 `[DONE]` 并写入完成记录。
- 提交前检查发现未跟踪文件 `notification.sh`、`run_agent.sh`，它们不是本任务产物，将保持未提交。
