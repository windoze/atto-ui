# 当前执行计划

本文件记录可检查的执行计划与进度，不包含隐藏推理过程。

## 计划

1. 读取 `TODO.md`，按标题是否包含 `[DONE]` 判断第一个未完成任务。
2. 检查该任务的依赖、验证要求和完成记录要求。
3. 在必要范围内查看相关代码与测试，避免进行无关历史问题扫查。
4. 按任务要求实现最小正确变更；若发现阻塞当前任务的具体前置问题，则更新 `TODO.md` 并停止。
5. 运行格式化、lint 和任务要求的相关测试；若发现未排期的测试/fixture 失败，修复或在 `TODO.md` 中排期。
6. 更新 `TODO.md`：在完成任务标题前加 `[DONE]`，并补充完成记录。
7. 如阶段计划确有变化才更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、近期提交，提交本次任务相关变更。
9. 完成一个任务后停止，不继续处理下一项。

## 进度

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
