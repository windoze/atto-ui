# 执行计划

## 目标

完成 `TODO.md` 中第一个标题未以 `[DONE]` 标记的任务，完成后更新任务记录、验证、提交 Git，然后停止。

## 步骤

1. 读取 `TODO.md`，按文件顺序确认第一个未完成任务。
2. 检查最近提交信息，仅判断是否存在与该任务直接相关的未完成事项；不做开放式历史问题扫查。
3. 阅读该任务相关代码和测试，明确验收要求与依赖。
4. 若存在阻塞当前任务的具体前置问题，按要求更新 `TODO.md` 并提交后停止；否则直接实现当前任务。
5. 使用小而聚焦的补丁修改代码、测试和文档。
6. 按要求运行验证：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后运行相关/完整测试。
7. 若发现未被显式排期的失败测试或夹具，修复它或将最小必要任务加入 `TODO.md` 的正确位置。
8. 任务完成后，在 `TODO.md` 的任务标题前加 `[DONE]`，更新完成记录。
9. 检查 `git status`、`git diff`、最近提交，确认只提交本轮相关变更。
10. 使用清晰提交信息提交所有本轮必要变更，然后停止，不进入下一项任务。

## 当前状态

- 已读取 `TODO.md`，第一个未完成任务是 `T8`：`L1 diagnostics gutter/statusbar 渲染与 F8 跳转`。
- 已读取 `TODO-2.md` 中 T8 详细要求；T8 依赖 T7，最近提交为 `[R7] Review LSP diagnostics state`，未发现直接相关的未完成阻塞项。
- 当前工作区有无关未跟踪文件 `notification.sh`、`run_agent.sh`，本任务不修改它们。
- 已定位 T8 实现路径：`EditorAction`/keymap 增加 F8/Shift+F8；`EditorView` 基于 editor-core diagnostics 做跳转与 gutter marker；`EditorTheme` 映射 LSP diagnostics style id；`DocumentTabView` 保留 primary handle summary；`EditorWindowView` 汇总 active tab summary；app on_tick 把 active/last-focused editor summary 写入旧 StatusBar custom 文本。
- 已完成主要代码修改：editor action/keymap、diagnostics gutter/style、F8 跳转；app active diagnostics summary/statusbar 接线；snapshot editor diagnostics 模式与相关单元/集成/PTY 测试已补充。
- `cargo fmt` 与 `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- 完整 `cargo test --workspace --all-targets` 发现 `pty_editor_triple_click_selects_line_and_replaces_on_type` 在 2 秒等待初始渲染时超时，但失败快照已包含目标文本；这是 PTY 并行负载下等待过短的测试稳健性问题。
- 已将 `pty_editor.rs` 文本等待统一为 5 秒；`cargo test -p atto-ui-editor --test pty_editor` 已通过。
- 重新运行 `cargo clippy --workspace --all-targets -- -D warnings` 与完整 `cargo test --workspace --all-targets` 均已通过。
- 已更新 `TODO.md` 和 `TODO-2.md`，将 T8 标记为 `[DONE]` 并写入完成记录。
- 已检查 `git status`、`git diff --stat`、`git log --oneline -10` 与 diff；无关未跟踪文件 `notification.sh`、`run_agent.sh` 不纳入提交。
- 下一步提交本轮 T8 相关变更后停止。
