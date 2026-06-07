# 当前执行计划

## 范围

- 以 `TODO.md` 为任务顺序和完成状态的权威来源。
- 只完成首个标题未标记 `[DONE]` 的任务，完成后停止。
- 本文件记录可公开的执行计划、关键决策和进度，不包含隐藏推理链。

## 执行步骤

1. 读取 `TODO.md`，识别第一个未完成任务及其验证要求。
2. 只检查最新提交是否有与当前任务直接相关的未完成事项。
3. 阅读当前任务涉及的代码、测试和完成记录要求。
4. 按任务要求做最小正确修改；若发现阻塞性规格缺口，修复或加入最小前置任务后停止。
5. 依次运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、完整测试。
6. 将完成任务在 `TODO.md` / 详细 TODO 文件中显式标记为 `[DONE]`，并更新完成记录。
7. 只提交当前任务相关文件，然后停止。

## 进度记录

- 已在任务检查前记录初始计划。
- 已识别首个未完成任务：`R2 — 审阅 T2`，位于 `TODO-2.md`。
- 审阅重点：`global_drag` 与 WM chrome `drag` 优先级、`Up`/`Esc`/window close 清理路径、overlay 绘制健壮性、测试是否经由 `WindowManager` 事件路径。
- 审阅发现一个当前任务范围内缺陷：`global_drag` active 时，Desktop menu/status bar 可能先消费鼠标 `Up`，导致 `WindowManager` 收不到释放事件并残留拖拽状态。
- 已调整计划：修复 Desktop 在全局拖拽期间先路由 mouse/`Esc` 到 `WindowManager`，并补充 chrome release 与 source/target close 清理测试。
- 已实现修复并新增测试：Desktop status bar release 清理全局拖拽；WM 关闭 source/target window 清理 `global_drag`。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- 已在 `TODO.md` 和 `TODO-2.md` 将 `R2` 标记完成并写入完成记录。
