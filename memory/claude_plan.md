# Claude 执行计划

## 范围

以 `TODO.md` 为权威任务列表，本轮只完成第一个未完成任务：`R12 — 审阅 T12`。本轮是审阅任务，重点确认 T12 的 OSC52 剪贴板、core 通用文本选区复制、测试覆盖和降级行为；如发现缺陷，优先修复与 R12 直接相关的问题，验证通过后更新 `TODO.md`、提交并停止。

## 步骤

1. 读取 `TODO.md`，确认第一个标题未带 `[DONE]` 的任务是 `R12`。
2. 检查最新提交信息；若明确提到与 R12 直接相关的未完成问题，将其纳入本轮审阅或作为前置记录到 `TODO.md`。
3. 审阅 T12 相关实现：`src/clipboard.rs`、core `Text` 选区逻辑、`TextBox` 剪贴板路径、test-host raw output 捕获、`snapshot_clipboard_app` 和 PTY/单测。
4. 对照 R12 验收点确认：OSC52 编码正确且写出失败安全降级；选区位于 core 通用层且未引入 editor 依赖；PTY 覆盖选区高亮与 OSC52 输出。
5. 如发现 R12 范围内的缺陷，做最小正确修复并补充测试；如发现阻塞但不能在本轮正确完成，更新 `TODO.md` 加入最小前置任务并停止。
6. 按要求验证：先 `cargo fmt`，再 `cargo clippy --workspace --all-targets -- -D warnings`，再运行相关 PTY 测试和完整 workspace 测试。
7. 将 `R12` 标题标记为 `[DONE]`，补充完成记录，必要时仅更新 `TODO.md`；除非阶段计划变化，否则不更新 `PLAN.md`。
8. 提交本轮相关更改并停止，不处理 `T13`。

## 进度

- 已读取 `TODO.md` 并确认第一个未完成任务：`R12 — 审阅 T12`。
- 已写入本轮 R12 审阅计划；下一步检查最新提交与 T12 相关实现。
- 已检查最新提交：最新提交仅记录 T12 completion plan；T12 实现提交为 `[T12] Add system clipboard selection`，未显示与 R12 直接相关的未完成事项。
- 已审阅 `src/clipboard.rs`、`src/composable/primitives.rs`、`src/widgets/textbox.rs`、`crates/atto-ui-test-host/src/lib.rs`、`src/bin/snapshot_clipboard_app.rs`、`tests/pty_clipboard.rs`。
- 当前审阅结论：未发现 R12 范围内缺陷；OSC52 编码路径正确，写出失败由调用点 best-effort 忽略；文本选区实现位于 core 通用 `Text`/`TextBox` 路径，未引入 editor 依赖；PTY 覆盖选区高亮与 OSC52 输出。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --test pty_clipboard`；`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`，将 R12 标记为 `[DONE]` 并记录审阅与验证结果。
- 下一步检查本轮 diff/status，提交 R12 相关文件后停止。
