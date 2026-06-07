# 执行计划

## 约束

- 以 `TODO.md` 为任务顺序和完成状态的唯一来源。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 若遇到阻塞当前任务的缺陷或缺失能力，优先修复；无法直接修复时，在 `TODO.md` 中加入最小前置任务并停止。
- 只在阶段级计划变化时更新 `PLAN.md`。
- 完成实现后按要求运行格式化、lint、测试，并提交 Git 变更。

## 步骤

1. 读取 `TODO.md`，定位第一个未完成任务，并确认其依赖、验证要求和完成记录格式。
2. 检查最近提交是否明确提到与该任务直接相关的未完成事项。
3. 根据任务内容阅读相关代码和测试，避免做无关历史问题排查。
4. 实现当前任务，保持改动最小且符合现有结构。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行相关测试；若需要完整验证，运行完整测试套件并设置足够超时。
6. 若发现未被安排的失败测试或阻塞缺陷，修复它；若不能在当前任务内正确修复，则更新 `TODO.md` 添加前置任务并停止。
7. 将当前任务标题标记为 `[DONE]`，更新完成记录，必要时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交记录，提交本次任务相关全部变更。
9. 停止，不继续处理后续任务。

## 进度

- 已写入初始执行计划。
- 已读取 `TODO.md` 与 `TODO-1.md`，第一个未完成任务为 `NR16 — 审阅 NT16`。
- 当前执行单元：审阅 `packages/react/__test__/reconciler_matrix.cjs` 是否覆盖 NT16 要求的 mount/update/增删/重排/事件 bind-clear、op 顺序与分桶断言，并运行要求的单测/验证。
- 已检查最新提交 `54ae1c8 [NT16] Record completion status`，提交摘要未声明与 NR16 直接相关的未完成事项。
- 审阅发现矩阵主体覆盖主要 `TreeOp`，但事件 handler 替换不重绑、`clearContainer` 回调释放、非尾部 move 三个边界主要由旧测试覆盖；计划将这些边界补进 `reconciler_matrix.cjs`，使矩阵自身满足 NR16 的覆盖要求。
- 已补充 `reconciler_matrix.cjs`：新增 handler 更新零 op/复用 callback、非尾部 move anchor、清空容器释放 callback 且 stale callback 不分发的断言。
- 初步验证通过：`npm run build --prefix packages/react && node packages/react/__test__/reconciler_matrix.cjs`。
- 下一步验证：按要求运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、完整 Rust 测试、React typecheck/test，并视结果修复。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`git diff --check`。
- 已确认仓库中没有 `tools/run_fixtures.py`，无单独 fixture 套件可运行。
- 下一步：更新 `TODO.md` / `TODO-1.md` 将 `NR16` 标记为 `[DONE]` 并写入完成记录，然后检查 diff 并提交。
- 已更新 `TODO.md` 与 `TODO-1.md`，`NR16` 已标记为 `[DONE]` 并写入完成记录。
- 已复查工作区状态、diff 和最近提交，仅暂存本次任务相关文件，未纳入无关未跟踪文件 `notification.sh`、`run_agent.sh`。
- 已提交 NR16 审阅变更：`e222e99 [NR16] Review reconciler test matrix`。
- 当前任务已完成，按要求停止，不继续 `NT17`。
