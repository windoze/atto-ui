# 当前执行计划

## 约束说明

- 本文件记录可公开的执行计划、关键决策和进度，不包含私密推理链路。
- `TODO.md` 是任务顺序和完成状态的唯一依据；只完成第一个标题未带 `[DONE]` 的任务。
- 如遇阻塞当前任务的缺陷或缺失能力，优先修复；若无法在当前任务内正确修复，则在 `TODO.md` 中加入最小必要前置任务并停止。
- 完成任务后必须更新 `TODO.md` 的标题和 completion record，运行要求的验证，提交 Git，然后停止。

## 初始步骤

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务，并读取该任务的具体要求、依赖和验证要求。
2. 检查最新提交信息是否明确提到与该任务直接相关的未完成问题；如相关，将其纳入当前任务或作为前置任务记录。
3. 读取任务涉及的源码、测试和文档，确定最小正确实现范围。
4. 按任务要求实施代码或文档变更，避免绕过规格或缩窄测试形状。
5. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后按任务要求运行相关或完整测试。
6. 若发现未被显式排期的测试/fixture 失败，修复它；若无法立即修复，则将最小前置任务加入 `TODO.md` 并停止。
7. 更新 `TODO.md`：将完成的任务标题加 `[DONE]`，补充 completion record，包括变更摘要与验证结果。
8. 如阶段级计划未变化，不更新 `PLAN.md`。
9. 检查 Git 状态和 diff，提交所有本任务相关变更，提交信息使用任务编号和简短说明。
10. 停止，不处理下一个任务。

## 进度记录

- 已读取 `TODO.md` 与 `TODO-2.md`，首个未完成任务为 `T1 — C1 通用拖拽数据模型与 Component hooks`。
- 已检查最新提交 `5d9e4d0 [NR20] Review CI runtime compatibility docs`，未发现与 T1 直接相关的未完成问题。
- 已新增 `src/composable/drag.rs`、`DragAndDrop` no-op trait、`ComponentContext.drag` 字段和 `composable` re-export。
- 已机械更新 `ComponentContext` 构造点为 `drag: None`，并开始根据 `cargo check --workspace --all-targets` 结果补齐未覆盖的手写组件 no-op `DragAndDrop` 实现。
- `cargo check --workspace --all-targets` 已通过，确认 trait 迁移覆盖当前 workspace、demos、examples 与 tests 构建目标。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test`。
- 已将 `TODO-2.md` 的 T1 标记为 `[DONE]` 并写入完成记录；已同步更新 `TODO.md` 索引状态。
- 下一步检查 Git 状态和 diff，确认只包含本任务相关变更，然后提交。
