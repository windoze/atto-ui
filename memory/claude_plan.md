# Claude 执行计划

## 范围

以 `TODO.md` 为权威任务列表，本轮只完成第一个未完成任务，验证通过后更新任务记录、提交本轮相关改动，然后停止。

## 步骤

1. 读取 `TODO.md`，识别标题未带 `[DONE]` 的第一个任务。
2. 只读取该任务所需的相关上下文；仅在阶段级依赖需要时读取或更新 `PLAN.md`。
3. 检查与当前任务直接相关的代码和测试。
4. 按任务原始要求实现，不缩窄范围、不引入 workaround。
5. 如发现阻塞当前任务的未排期前置问题，更新 `TODO.md` 加入最小前置任务，保持当前任务未完成，提交后停止。
6. 按顺序验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关测试和完整测试。
7. 将当前任务标题标记为 `[DONE]`，并补充完成记录。
8. 检查 `git status`、`git diff` 和最近提交；只提交本轮任务相关文件。
9. 停止，不处理下一项任务。

## 进度

- 已在选择任务前记录执行计划。
- 已识别第一个未完成任务：`T12 — 系统剪贴板 + 文本选区复制（core）`。
- 已确认最新提交为 R11，未包含与 T12 直接相关的未完成事项。
- 已确认现状：TextBox 只有应用内剪贴板；core `Text` 没有渲染文本选区复制；项目没有 OSC52 剪贴板模块。
- 已实现 `src/clipboard.rs` 的 std-only OSC52 编码/写出能力。
- 已为 core `Text` 增加 opt-in `selectable` 与 `clipboard` binding，支持鼠标拖选、跨行选区高亮、Ctrl+C 写入 binding 并发出 OSC52。
- 已更新 TextBox 复制/剪切路径，使其保留原有内部 binding 行为并同步发出 OSC52。
- 已为 `atto-ui-test-host` 增加 raw PTY output 捕获与 `wait_for_output`，用于断言 OSC52 等不可见控制序列。
- 已新增 `snapshot_clipboard_app` 与 `tests/pty_clipboard.rs`，覆盖跨行拖选、选区高亮和 OSC52 输出。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --test pty_clipboard`；`cargo test --workspace --all-targets`。
- 已将 T12 在 `TODO.md` 中标记为 `[DONE]` 并写入完成记录。
- 当前正在只检查和提交 T12 相关文件；保留工作树中其他既有无关变更不动。
