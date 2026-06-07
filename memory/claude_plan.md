# 执行计划

## 范围

- 以 `TODO.md` 为任务顺序和完成状态的权威来源。
- 只选择首个标题未标记 `[DONE]` 的任务；完成该任务后停止。
- 若发现阻塞当前任务的规格缺口或未排期失败，先修复，或添加最小前置任务后停止。
- 完成时更新 `TODO.md` / 详细 TODO 文件、记录验证结果，并提交当前任务相关改动。

## 步骤

1. 读取 `TODO.md`，定位首个未完成任务及其验证要求。
2. 只检查最近提交中与该任务直接相关的未完成事项。
3. 阅读任务涉及的 `Window`、`WindowManager`、Desktop 路由、re-export 与测试代码。
4. 实现 T3 的 dock public API、dock layout、effective work area、绘制/事件/placement 接入。
5. 增加 manager 单测覆盖 dock reserve、dock rect 覆盖、普通窗口 maximize/move/resize 与 auto-hide invisible reserve。
6. 依次运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
7. 将 T3 在 `TODO.md` 和 `TODO-2.md` 显式标记 `[DONE]` 并写入完成记录。
8. 查看 git 状态和 diff，只提交当前任务相关文件。

## 当前状态

- 首个未完成任务已确定为 `T3 — C2 Docking 类型、work area reserve 与基础绘制`。
- 最新提交 `[R2] Review global drag cleanup` 未显示与 T3 直接冲突的未完成问题。
- 已实现 T3：新增 `DockSide`、`DockAutoHide`、`WindowDock`、`Window.dock`、`Window::with_dock`、`WindowDock::docked`，新增 `src/wm/manager/docking.rs`，并将 add/draw/dispatch/drag/drop/move/resize/maximize 路径接入 dock-aware effective work area。
- 已补充单测：left/right/bottom dock reserve、dock rect 忽略原始 builder rect 且可绘制到边缘、普通窗口 move/resize clamp、auto-hide invisible 只 reserve 1 cell。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- `TODO.md` 与 `TODO-2.md` 已将 T3 标记为 `[DONE]` 并写入完成记录。
- 提交前检查发现未跟踪文件 `notification.sh`、`run_agent.sh`，它们不是本任务产物，将保持未提交。
