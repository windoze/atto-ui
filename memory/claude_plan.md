# 执行计划

## 约束

- 以 `TODO.md` 为唯一任务顺序与完成状态来源。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 不进行开放式历史问题清扫；只处理阻塞当前任务或当前验证暴露且未被明确排期的失败。
- 不写入私密推理链；本文件记录可审查的执行计划、关键决策和进度。

## 步骤

1. 读取 `TODO.md`，按文档顺序识别第一个未完成任务。
2. 查看最近提交信息；仅当它明确提到与当前任务直接相关的未完成事项时，将其纳入当前任务或补为前置任务。
3. 阅读当前任务涉及的代码、测试和说明，确认验收要求。
4. 按最小正确变更实现任务；如遇必须先修复的具体阻塞问题，更新 `TODO.md` 插入最少前置任务并停止。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再按任务要求运行相关测试；需要时运行完整测试套件。
6. 若发现未排期的测试或 fixture 失败，修复或在 `TODO.md` 中排期到当前任务完成前。
7. 更新 `TODO.md`：给完成任务标题添加 `[DONE]`，填写完成记录和验证结果。仅在阶段计划变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交，提交本次任务相关所有未提交文件。
9. 停止，不继续下一个任务。

## 当前状态

- 已读取 `TODO.md` 与 `TODO-1.md`。
- 首个未完成任务确认为 `NT16 — reconciler 单测矩阵（T.1）`。
- 最近提交为 `[NR15] Review core imperative builders`，未发现与 NT16 直接相关的未完成事项。
- 已新增 `packages/react/__test__/reconciler_matrix.cjs` 并接入 `packages/react/package.json` 的 `npm test`。
- 矩阵覆盖 mount/set_tree、props set/clear、事件 bind/clear、文本更新、insert_before append/anchor、已挂载节点 move、remove 前 clear_event、clearContainer 空树、desktop 多窗口 op 分桶和窗口关闭不进 TreeOp。
- 首次运行新增矩阵测试发现测试用例把文本值同时作为 React key，导致多窗口分桶更新被解释为 remove/insert；已改用稳定 key，使该用例验证属性更新与分桶顺序。
- React build + 新增矩阵测试已通过。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`、`npm run typecheck --prefix packages/core`、`npm run typecheck --prefix packages/react`、`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`、`npm test --prefix crates/atto-ui-node`、`npm test --prefix packages/core`、`npm test --prefix packages/react`、`git diff --check`。
- 已更新 `TODO.md` 与 `TODO-1.md`，将 NT16 标记为 `[DONE]` 并写入完成记录。
- 补强事件清理断言后，已重跑 `npm test --prefix packages/react` 与 `git diff --check`，均通过。
- 提交前检查发现工作区另有未跟踪 `notification.sh`、`run_agent.sh`，它们不是本任务变更，未纳入提交。
- 已提交本任务实现：`c1fd6b8 [NT16] Add reconciler test matrix`。
- 当前任务完成后停止，不继续 `NR16`。
