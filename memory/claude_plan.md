# 当前执行计划

本文件记录可检查的执行计划与进度，不包含隐藏推理过程。

## 历史进度（上一轮 T15）

- 已创建初始执行计划，下一步读取 `TODO.md` 确认第一个未完成任务。
- 已读取 `TODO.md`，第一个未完成任务为 `T15 — 多行输入 + 历史 + 键盘增强（core）（C.3）`。
- 下一步检查最近提交是否有与 T15 直接相关的未完成事项，然后查看 TextBox/TextBuffer、事件循环键盘增强和 chat 输入面板的实现。
- 已检查最近提交 `R14`，未发现与 T15 直接相关的未完成事项。
- 实施方案：新增 core `TextArea` 组件；`CrosstermAppConfig`/终端 session 增加可关闭的键盘增强标志推送与 drop 恢复；chat 文本模式改用 `TextArea` 并持有输入历史；新增 chat PTY 覆盖 Shift+Enter、历史上下翻和 kill-ring。
- 已完成初稿并运行 `cargo fmt`；`cargo test -p atto-ui textarea` 通过。下一步运行新增 chat PTY 测试并修复交互问题。
- 新增 chat PTY 用例已通过；修正了测试输入与 fixture 命令键冲突。下一步按要求运行格式化、clippy 与测试套件。
- 第一次完整测试中 `chat_auto_follow_pauses_after_user_scrolls_up` 因 TextArea 增高改变 fixture 视口而失败；已将 chat snapshot 窗口高度同步加大并确认该用例单独通过。下一步重新运行 clippy 与完整测试。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。下一步更新 `TODO.md` 将 T15 标记为完成并写入完成记录。
- `TODO.md` 已将 T15 标记为 `[DONE]` 并写入完成记录。下一步检查 git 状态/diff/log，确认只提交本任务相关变更。

## 当前任务（R15）

### 计划

1. 读取 `TODO.md`，按标题是否包含 `[DONE]` 判断第一个未完成任务。
2. 检查该任务的依赖、验证要求和完成记录要求。
3. 在必要范围内查看相关代码与测试，避免进行无关历史问题扫查。
4. 审阅 T15 的键盘增强启用/恢复、降级路径、core `TextArea` 边界和 chat 接入方式。
5. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关 PTY/单测与完整 workspace 测试。
6. 更新 `TODO.md`：在 R15 标题前加 `[DONE]`，并补充完成记录。
7. 如阶段计划确有变化才更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、近期提交，提交本次任务相关变更。
9. 完成一个任务后停止，不继续处理下一项。

### 进度

- 已读取 `TODO.md`，第一个未完成任务为 `R15 — 审阅 T15`。
- 已检查最近提交 `89bc9b0 [T15] Add multiline TextArea input`，提交信息未提到与 R15 直接相关的未完成事项。
- 已审阅 `src/app/run.rs`、`crates/atto-ui-async/src/stream.rs`、`src/widgets/textarea.rs`、`crates/atto-ui-chat/src/input.rs` 与 `snapshot_chat_app` 接入；未发现 R15 范围内需要修复的代码缺陷。
- 确认同步/async `TerminalSession` 默认推送 `DISAMBIGUATE_ESCAPE_CODES`，写出失败时按未激活处理并继续运行，成功启用后在 `Drop` 中执行 `PopKeyboardEnhancementFlags`。
- 确认 `TextArea` 位于 core widgets，chat 仅通过 `atto_ui::widgets::TextArea` 组合消费；无法区分 `Shift+Enter` 的终端可用 `Ctrl+J` 换行降级。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui textarea`；`cargo test -p atto-ui-chat --test pty_chat chat_textarea_multiline_history_and_kill_ring`；`cargo test --workspace --all-targets`。
- `TODO.md` 已将 R15 标记为 `[DONE]` 并写入完成记录。下一步检查最终状态并提交本次 R15 相关变更。
- 已提交 R15 审阅与完成记录：`94b88c7 [R15] Review multiline TextArea input`。本次追加仅记录提交结果；因只修改执行记录文档，不需要重新运行测试。
