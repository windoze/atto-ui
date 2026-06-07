# 当前执行计划

## 约束说明

- 本文件记录可公开的执行计划、关键决策和进度，不包含私密推理链路。
- `TODO.md` 是任务顺序和完成状态的唯一依据；只完成第一个标题未带 `[DONE]` 的任务。
- 若遇到阻塞当前任务的缺陷、规格偏差或未排期测试失败，优先修复；若无法正确修复，则在 `TODO.md` 中加入最小必要前置任务并停止。
- 完成任务后必须更新 `TODO.md` / `TODO-2.md` 的标题与完成记录，运行任务要求的验证，提交 Git，然后停止。

## 当前任务

- 首个未完成任务：`R1 — 审阅 T1`。
- 来源：`TODO-2.md` 阶段一。
- 审阅范围：`T1 — C1 通用拖拽数据模型与 Component hooks` 的实现质量、正确性、公开 API 和测试覆盖。

## 执行步骤

1. 检查最新提交信息和当前工作区状态，确认是否存在与 R1/T1 直接相关的未完成事项或未提交续作。
2. 审阅 T1 涉及文件：`src/composable/drag.rs`、`src/composable/mod.rs`、`src/composable/component.rs`，以及 workspace 中所有 `ComponentContext` 构造点和手写 `impl Component` 类型。
3. 核对 R1 验收点：`DragAndDrop` 默认 no-op、所有 `ComponentContext` 构造点显式设置 `drag`、没有不安全占位或 panic、public re-export 不泄露 wm 内部私有类型。
4. 若发现 R1 范围内的实际缺陷，直接做最小正确修复并补充/调整测试；若发现无法立即修复的前置问题，将其插入 `TODO.md` / `TODO-2.md` 并停止。
5. 运行要求的验证：`cargo check --workspace --all-targets`、`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`；如代码有实质变更，再按需要运行相关测试。
6. 根据审阅结果更新 `TODO-2.md`：将 `R1` 标题改为 `[DONE] R1 — 审阅 T1`，写入完成记录；同步更新 `TODO.md` 索引状态。
7. 如阶段级计划没有变化，不更新 `PLAN-2.md`。
8. 检查 Git 状态和 diff，提交本任务相关变更，提交信息使用 `[R1] Review T1 drag and drop hooks`。
9. 停止，不处理 `T2`。

## 进度记录

- 已读取 `TODO.md` 和 `TODO-2.md`，确认当前任务为 `R1 — 审阅 T1`。
- 已检查最新提交 `7b67168 [T1] Add drag and drop component hooks`，该提交正是 R1 审阅对象；当前工作区另有未跟踪的 `notification.sh`、`run_agent.sh`，与本任务无关且不触碰。
- 已审阅 T1 涉及的拖拽类型、`DragAndDrop` 默认 hook、`ComponentContext.drag` 构造点、`composable` re-export 与手写 `impl Component` 补齐情况；未发现需要代码修复的问题。
- 验证已通过：`cargo fmt`；`cargo check --workspace --all-targets`；`cargo clippy --workspace --all-targets -- -D warnings`。
- 已将 `TODO-2.md` 的 R1 标记为 `[DONE]` 并写入完成记录；已同步更新 `TODO.md` 索引状态。
- 已检查 Git diff，确认本次仅需提交 `TODO.md`、`TODO-2.md` 和 `memory/claude_plan.md`；未跟踪脚本与 R1 无关，不纳入提交。
- 下一步提交 R1 变更，然后停止。
