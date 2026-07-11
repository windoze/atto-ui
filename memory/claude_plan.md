# 执行计划

## 约束
- 以 `TODO.md` 为任务顺序和完成状态的唯一依据。
- 只处理首个标题未带 `[DONE]` 的任务，完成后停止。
- 任务完成前需按要求验证，更新 `TODO.md` 完成记录，并提交 Git。
- 不在此文件记录私有推理；仅记录可审计的执行计划与进度。

## 步骤
1. 读取 `TODO.md`，确认首个未完成任务及其依赖、验收要求和完成记录格式。
2. 检查当前仓库状态和相关文件，避免覆盖无关改动。
3. 实施该任务需要的最小正确代码或文档变更。
4. 按顺序运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行必要的完整测试。
5. 如发现未调度的测试失败或阻塞问题，修复它；若无法在当前任务内完成，则在 `TODO.md` 插入最小前置任务并停止。
6. 验证通过后，在 `TODO.md` 中给当前任务标题添加 `[DONE]` 并填写完成记录。
7. 检查 diff，提交本次任务相关的全部未提交变更，然后停止。

## 当前进度
- 已创建初始执行计划。
- 已确认首个未完成任务为 `M4.4 剪贴板（首版）`。
- 已检查最新提交与工作区状态，最新提交 `[M4.3] Implement terminal copy mode` 与当前任务相关。
- 已实现内部 copy buffer 首版接线：鼠标选区释放后写入 buffer，copy-mode 继续通过 `y/Enter` 写入 buffer，`prefix+]` 与 handle API 将 buffer 按 bracketed paste 规则粘贴回输入流。
- 已补充单测与 PTY 测试，并完成验证：目标单测、目标 PTY、`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo test --workspace --all-targets` 均通过。
- 已在 `TODO.md` 将 `M4.4` 标记为 `[DONE]` 并填写完成记录。
- 下一步检查 diff 并提交本次任务变更。
