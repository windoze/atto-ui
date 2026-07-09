# 当前执行计划

1. 读取 `TODO.md`，严格按文件顺序识别第一个标题未带 `[DONE]` 的任务。
2. 查看该任务的要求、依赖、验证方式和完成记录；只在与当前任务直接相关时检查最新提交或相邻上下文。
3. 检查工作区状态，避免覆盖用户或其他代理的未提交改动。
4. 基于当前任务定位相关代码和测试，实施最小且完整的修复或功能实现。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后在需要时运行完整测试套件。
6. 若遇到阻塞当前任务且无法直接修复的前置问题，更新 `TODO.md` 加入最小必要前置任务并停止；否则完成当前任务。
7. 将当前任务标题标记为 `[DONE]`，更新其完成记录；仅在阶段计划变化时更新 `PLAN.md`。
8. 检查变更，提交包含本次任务相关文件的 Git commit，然后停止，不继续下一项任务。

当前任务：`P5.4 快照与测试`。

任务要求：补充 PTY 覆盖搜索命中跳转（含屏外）、turn 折叠/展开、引用附加与移除。只完成这一项任务，完成后更新 `TODO.md` 并提交。

进度：已确认第一个未完成任务为 P5.4；工作区除本计划文件外无其它未提交改动；最近提交为 P5.3 完成记录，未显示与 P5.4 直接相关的未完成前置问题。

进度：已定位 `snapshot_chat_app.rs` 与 `pty_chat.rs`。现有列表组件已有 Ctrl+R 搜索、turn header 的 `Collapse`/`Expand` 控件，以及启用 `with_quote_replies` 后的 `Quote` / `Quote block` 操作和输入区引用栏。

进度：已在 `snapshot_chat_app` 增加 `--p5-search` 与 `--p5-fold-quote` fixture，并让它们避开默认演示快捷键和加载更多回调。`--p5-fold-quote` 已启用 `with_quote_replies`。

进度：已在 `pty_chat.rs` 增加 P5.4 PTY 用例，覆盖搜索从底部跳到屏外首个命中并切到下一个命中、turn 折叠/展开隐藏和恢复块内容、turn/block 引用 attach 后从输入引用栏移除。

进度：`cargo fmt --all` 通过。新增 PTY 首轮发现测试交互需要调整：默认焦点在输入框，`Ctrl+R` 被输入层处理而没有进入列表搜索；引用栏点击 `Remove` 文本没有命中实际移除区域。

进度：搜索 PTY 在先聚焦消息列表后通过；引用移除仍失败，定位为组件真实问题：引用栏记录绝对绘制区域，而父布局传给 `ChatInputPanel` 的鼠标事件是本地坐标。已修复 `ChatInputPanel` 将本地坐标转换回绝对坐标后再命中引用移除区域，并新增本地坐标单测。

进度：`cargo fmt --all`、`cargo test -p atto-ui-chat reference_remove_click_handles_local_mouse_coordinates --lib`、`cargo test -p atto-ui-chat --test pty_chat chat_p5 -- --nocapture` 均通过。

进度：通用验收命令已全部通过：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。

进度：`TODO.md` 已将 P5.4 标记为 `[DONE]`，并写入 fixture、PTY 覆盖、引用栏坐标修复和验证命令的完成记录。

当前步骤：检查 `git status`、`git diff`、`git log --oneline -10`，确认只提交本次任务相关变更，然后创建提交并停止。
