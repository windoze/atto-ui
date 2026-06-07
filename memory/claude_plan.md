# 执行计划

## 当前目标

按照 `TODO.md` 的顺序完成第一个未标记 `[DONE]` 的任务：`R7 — 审阅 T7`，并在完成验证后更新任务记录、提交 Git，然后停止。

## 执行步骤

1. 读取 `TODO.md`，只识别第一个标题未带 `[DONE]` 的任务，不做开放式问题扫描。
2. 查看该任务的要求、依赖、验证方式和完成记录；必要时查看 `PLAN.md` 或最近提交是否有与该任务直接相关的未完成事项。
3. 检查当前工作区状态，避免覆盖用户或其他代理的无关改动。
4. 基于任务要求读取相关代码和测试，确认最小正确实现范围。
5. 按任务要求实现变更；如果发现阻塞当前任务的真实前置问题，优先修复，或把最小前置任务插入 `TODO.md` 后提交并停止。
6. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后按任务要求运行相关测试；若代码发生影响编译行为的变更，运行完整测试套件。
7. 修复验证中发现且未被明确排期的测试或 fixture 失败，不以既有失败为理由跳过。
8. 在 `TODO.md` 中给完成的任务标题加 `[DONE]`，补充完成记录和验证结果；仅当阶段计划变化时更新 `PLAN.md`。
9. 提交所有与本次任务相关的变更，提交信息包含任务编号和清晰描述。
10. 停止，不继续处理下一个任务。

## 进度记录

- 已创建初始计划。
- 已读取 `TODO.md`，第一个未完成任务是 `R7 — 审阅 T7`。
- 已读取 `TODO-2.md` 中 T7/R7 详情；R7 审阅重点是 LSP diagnostics 事件处理、uri/version 过滤、summary binding 更新、LSP session 出错时清理 diagnostics/style layer。
- 最近提交 `6aa5374 [T7] Add LSP diagnostics state model` 与当前审阅任务直接相关，将作为主要审阅对象。
- `PLAN.md` 不存在；本次暂无阶段计划变更，除非审阅发现结构性依赖变更，否则不更新阶段计划文件。
- 工作区存在未跟踪 `notification.sh`、`run_agent.sh`，它们不是本次任务产物，除非后续发现直接相关，否则不修改、不提交。
- 审阅发现 `clear_lsp_diagnostics` 使用带副作用 `.take()` 的短路 `||`，在已有 diagnostics 时会跳过后续字段清理；已修复为先分别 `take` 再合并状态，并补充单元回归测试。
- `cargo clippy` 首次运行暴露测试无法访问私有 `clear_lsp_diagnostics`；已将该 helper 调整为 `pub(super)`，仍限制在 `view` 模块内部。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 与 `TODO-2.md` 中 `R7 — 审阅 T7` 标记为 `[DONE]`，并写入完成记录。
- 下一步只剩检查 diff/status 并提交本次 R7 变更；提交后停止，不处理 T8。
