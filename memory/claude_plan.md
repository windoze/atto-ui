# T4 执行计划

## 当前任务

- `T4 — DesktopInspector 快照导出（B.1）` 是 `TODO.md` 中第一个未完成任务。
- 目标是在纯内存 `AppHost` 路径暴露可序列化的 UI 快照，包含组件树、组件 id/tag/type、bounds、文本内容，并补充单测。

## 思路摘要

- 先核对最近一次提交是否包含与 T4 直接相关的未完成事项。
- 以现有 `DesktopInspector`、动态窗口树和 `AppHost` API 为基础做最小改动，不引入 PTY 依赖。
- 快照结构应稳定、无环、可 `serde` 序列化，且字段足以支撑后续 Python e2e 断言。
- 如发现阻塞 T4 的既有缺口，先修复；若无法在本次任务内正确修复，则按要求把最小前置任务加入 `TODO.md` 后提交并停止。

## 执行步骤

1. 检查最近提交、工作区状态和 T4 相关代码：`src/inspect.rs`、`src/app/run.rs`、动态窗口/组件树/文本属性相关实现。
2. 设计并实现 snapshot 数据结构，优先放在 `src/inspect.rs`，并通过 `src/lib.rs` 或现有公开路径导出。
3. 在 `AppHost` 增加 `snapshot()`，从内存中的 `Desktop`/`DesktopInspector` 获取快照，不依赖真实 PTY。
4. 补充单测：构建小窗口树，执行必要 `step`/layout 后断言快照包含预期组件 id、tag/type、bounds 与文本。
5. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`，通过后运行完整测试 `cargo test`。
6. 更新 `TODO.md`：给 T4 标记 `[DONE]`，填写完成记录和验证结果；仅在阶段计划确实变化时才更新 `PLAN.md`。
7. 提交所有本任务相关改动，提交信息使用 `[T4] ...`，然后停止。

## 进度记录

- 已读取 `TODO.md` 并确认第一个未完成任务为 T4。
- 已检查最近提交：`[R3] Review AppHost event injection APIs`，未发现直接阻塞 T4 的未完成事项。
- 已检查工作区状态：存在多处非本次任务的既有改动；后续只编辑并提交 T4 相关文件。
- 已初步阅读 `src/inspect.rs` 与 `src/app/run.rs`，现有 inspector 可渲染 `TestBackend` 并构建内部 `InspectNode`，但对外 `AppHost` 尚无可序列化 snapshot API。
- 开始实现方案：新增独立可序列化快照类型，保留既有 `InspectSnapshot` buffer 调试能力；`AppHost::snapshot()` 委托纯内存 inspector 生成结构化快照。
- 已完成初版实现：`DesktopSnapshot`/`DesktopSnapshotNode`、`DesktopInspector::export_snapshot()`、`AppHost::snapshot()` 与覆盖 id/tag/type/bounds/text/state/serde 的单测。
- 已运行 `cargo fmt` 与聚焦测试 `cargo test -p atto-ui inspect::tests::export_snapshot_contains_serializable_tree_bounds_and_text`，聚焦测试通过。
- 已运行 `cargo clippy --workspace --all-targets -- -D warnings`、`cargo test`、`cargo test --workspace --all-targets`，均通过；buffer clone 优化后已重新运行并通过同一组验证。
- 已更新 `TODO.md`：T4 标记为 `[DONE]`，并写入实现与验证完成记录。
- 提交前复查发现 `export_snapshot()` 可避免克隆完整 buffer；已调整为仅 `InspectSnapshot` 路径在需要 buffer 时克隆，`DesktopSnapshot` 路径只做内存绘制刷新布局。
