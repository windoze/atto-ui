# 执行计划

## 约束说明
- 我会按 `TODO.md` 的顺序只处理第一个标题未带 `[DONE]` 的任务。
- 我会记录可检查的执行计划、关键决策和进度，但不会写入隐藏推理细节。
- 若发现阻塞当前任务的未跟踪前置问题，会先更新 `TODO.md` 并提交后停止。

## 初始步骤
1. 读取 `TODO.md`，定位第一个未完成任务，并检查该任务的依赖、验收条件和完成记录。
2. 查看最近提交信息；如果它明确提到与当前任务直接相关的未完成问题，则纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 只读取与当前任务相关的代码、测试和文档，避免无关的历史问题清扫。

## 实施步骤
1. 根据当前任务要求做最小且完整的代码或文档修改。
2. 如遇到实现缺口、规格不匹配或测试/fixture 失败，优先修复；如果不能在当前任务中正确完成，则在 `TODO.md` 中添加最小前置任务并停止。
3. 运行格式化、lint 和相关测试；若需要全量验证，按要求使用足够长的超时时间。
4. 更新 `TODO.md`：任务完成时在标题加 `[DONE]` 并填写完成记录；仅在阶段级计划变化时更新 `PLAN.md`。
5. 检查工作区差异，提交本次任务涉及的所有必要变更。
6. 提交后停止，不继续处理下一个任务。

## 当前状态
- 已读取 `TODO.md`，第一个未完成任务是 `T14D — 拆分 window manager 巨型文件（M8）`。
- 最近提交为 `[T14C] Split runtime module`，是当前任务的直接依赖完成记录，未发现需要先处理的相关未完成事项。
- 下一步：检查工作区状态并阅读 `src/wm/manager/mod.rs` 及相邻模块，按职责拆分为子模块，保持公开 API 与行为不变。
- 工作区状态已检查：仅 `memory/claude_plan.md` 为本次修改；`notification.sh`、`run_agent.sh` 为既有未跟踪文件，本任务不会触碰。
- 已阅读 `src/wm/manager/mod.rs` 与现有 `chrome/draw/events/focus/placement/z_order` 子模块。
- 拆分方案：新增 `types.rs` 放置 WindowManager 相关类型与内部拖拽/命中状态，新增 `core.rs` 放置构造、窗口增删、动态树操作等核心方法，新增 `tests.rs` 放置原 `mod.rs` 单元测试；`mod.rs` 保留模块声明与公开 re-export。
- 已完成机械拆分并运行 `cargo fmt`。
- 小范围验证：`cargo test --lib window_manager` 与 `cargo test --lib wm::manager::tests` 均通过，确认拆出的 manager 单元测试仍可运行。
- 验证已完成：`cargo clippy --workspace --all-targets -- -D warnings` 通过；`cargo test --all --all-targets` 通过。
- 下一步：更新 `TODO.md` 的 T14D 标题与完成记录，然后检查 diff 并提交。

## T14D 任务计划
1. 检查当前 git 工作区状态，避免覆盖他人或既有未提交改动。
2. 阅读 `src/wm/manager/mod.rs` 和 `src/wm/manager/` 现有子模块，识别适合拆出的职责边界。
3. 采用最小机械重构：把类型定义、构造/窗口管理、分派/绘制/焦点/z-order 等实现移入职责明确的子模块，`mod.rs` 保留 facade、模块声明和公开 re-export。
4. 每次拆分后运行小范围编译或测试来捕获可见性/导入问题，修复后继续。
5. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
6. 更新 `TODO.md`：将 T14D 标记 `[DONE]` 并写入完成记录。
7. 检查 diff 与状态，提交本任务变更后停止。
