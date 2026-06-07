# 执行计划

## 当前约束

- `TODO.md` 是任务顺序与完成状态的唯一索引；任务标题未带 `[DONE]` 就视为未完成。
- 本轮只完成第一个未完成任务，完成后提交并停止。
- 不做开放式历史问题扫描；只审阅与当前任务直接相关的实现、测试与最新提交。
- 如发现当前任务被具体缺陷阻塞，先修复；无法直接修复时在 `TODO.md` 插入最小必要前置任务后提交并停止。
- 代码变更后按顺序验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。

## 执行步骤

1. 读取 `TODO.md`，定位第一个未完成任务。
2. 读取任务详情来源 `TODO-2.md`，确认 R4 审阅范围和验收关注点。
3. 检查最新提交是否与当前任务直接相关，并限定审阅范围。
4. 审阅 T4 的 dock resize、auto-hide、hit-test 和绘制路径。
5. 对审阅发现的测试覆盖缺口补充最小回归测试。
6. 运行格式化、clippy 和完整 workspace 测试。
7. 在 `TODO.md` / `TODO-2.md` 标记 R4 为 `[DONE]` 并写入完成记录。
8. 检查最终 diff、提交本轮变更，然后停止。

## 进度记录

- 已在读取 `TODO.md` 前写入初始执行计划。
- 已确认第一个未完成任务为 `R4 — 审阅 T4`，最新提交为 `[T4] Add dock resize and auto-hide hit-test`，审阅范围限定为 T4 实现与测试。
- 已审阅 T4 docking 路径，未发现需要修改实现的规格偏离。
- 已补充 R4 定向回归测试，覆盖四个 `DockSide` 的 resize edge、min/max/available clamp、hidden auto-hide 不绘制 view、visible overlay 鼠标不穿透 normal window。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- 已在 `TODO.md` 和 `TODO-2.md` 将 R4 标记为 `[DONE]` 并记录完成情况；下一步检查最终 diff 并提交。
