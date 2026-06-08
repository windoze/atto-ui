# 执行计划

## 当前状态

- 已读取 `TODO.md` 和 `TODO-2.md`，第一个未完成任务为 `T15 — File picker 与 Buffer/tab picker`。
- `PLAN.md` 在仓库根目录不存在；相关阶段计划位于 `PLAN-2.md`，已查看 file picker / buffer picker 章节。
- 最新提交为 `[R14] Review command palette picker`，未发现直接提示 T15 的未完成阻塞事项。
- 已完成核心实现草稿：新增 workspace file index、`OpenFilePicker` / `OpenBufferPicker` / `SelectEditorTab` actions、file/buffer picker modal 事件流、tab stable id 和选择命令。
- 已增加单元与 PTY 测试草稿，覆盖 file picker cache invalidation、`Ctrl+P` 打开 workspace 文件、buffer picker stable tab id selection。
- 已运行 `cargo fmt`。
- 已运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过。
- 已运行 `cargo test --workspace --all-targets`，通过。
- 已将 T15 在 `TODO.md` / `TODO-2.md` 标记为 `[DONE]` 并填写完成记录。
- 已提交本任务变更，提交为 `[T15] Implement file and buffer pickers`。
- 本文件用于记录可审阅的执行计划、关键决策、进度和验证结果。

## 步骤

1. 阅读 T15 相关文件：`actions.rs`、`app.rs`、`workspace.rs`、`window.rs`、`window/tabs.rs`、`picker.rs`，确认现有 command palette、open path、workspace tree、tab 状态和测试模式。
2. 设计并实现 `AppAction::OpenFilePicker`、`OpenBufferPicker`、`SelectEditorTab { window, tab_id }`，复用现有 action 分发路径。
3. 实现 file picker：基于 workspace roots / `build_workspace_tree` flatten file nodes，排除目录和隐藏 `.git` 内容，加入缓存和 roots invalidation，accept 时调用 `OpenPath { target: NewTab }`。
4. 实现 buffer/tab picker：为 `TabState` 增加 stable `tab_id`，暴露 tab summaries，增加 `EditorWindowCommand::SelectTabById(u64)`，accept 后按窗口和 tab id 切换。
5. 接入快捷键与命令：`Ctrl+P` 打开 file picker；buffer picker 通过 command palette 命令暴露。
6. 增加或更新单元/PTY 测试，覆盖 file picker fuzzy 打开 `src/main.rs`、buffer picker 在两个 tabs 间按 stable id 切换。
7. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
8. 更新 `TODO.md` / `TODO-2.md` 完成记录并提交本任务变更后停止。
