# 当前执行计划

## 原则

- 以 `TODO.md` 为唯一任务排序和完成状态来源。
- 本轮只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 若遇到阻塞当前任务的真实缺陷或缺失能力，优先修复；若无法在本轮正确修复，则在 `TODO.md` 中插入最小必要前置任务并提交后停止。
- 不用 workaround、夹具特化或缩窄范围替代规格正确实现。
- 修改 `PLAN.md` 仅限阶段级计划或依赖发生变化。

## 步骤

1. 读取 `TODO.md`，确认第一个未完成任务及其验证要求。
2. 查看最近提交信息，仅判断是否有明确未完成且直接相关的事项。
3. 根据任务内容读取最小必要代码和测试上下文。
4. 实现当前任务或处理其直接前置阻塞问题。
5. 按要求运行格式化、lint 和相关/完整测试；若发现未排期失败，修复或在 `TODO.md` 中排入必要任务。
6. 更新 `TODO.md`：将完成任务标题加 `[DONE]`，补充完成记录；必要时更新 `PLAN.md`。
7. 检查 `git status`、`git diff`、最近提交，暂存本轮相关改动并提交。
8. 停止，不进入下一个任务。

## 进度

- 已创建本轮执行计划。
- 已读取 `TODO.md` 与 `TODO-1.md`，确认本轮第一个未完成任务为 `NR5 — 审阅 NT5`。
- 已查看最近提交，最新提交为 `NR4`，未发现直接声明 `NR5` 相关未完成事项。
- 已审阅 `packages/core` 的入口、native loader、类型文件与 smoke/type 测试，未在对外 `index.ts` 中发现 `any` 泄漏。
- 已运行并通过 `npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`。
- 已运行并通过 `node packages/core/__test__/headless.cjs` 与 `npm test --prefix packages/core`。
- 已运行并通过 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
- 已将 `TODO-1.md` 的 `NR5` 标记为 `[DONE]` 并补充完成记录；已同步更新 `TODO.md` 索引状态。
- 下一步检查 diff/status，暂存本轮相关改动并提交。
