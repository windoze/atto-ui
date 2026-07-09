# 执行计划

## 范围
- 以 `TODO.md` 为唯一任务来源，识别第一个标题未带 `[DONE]` 的任务。
- 本轮只完成一个任务；完成后更新 `TODO.md`、验证、提交并停止。
- 如遇阻塞，只添加最小必要的前置任务并提交，不继续执行后续任务。

## 步骤
1. 读取 `TODO.md`，确定第一个未完成任务及其验证要求。
2. 检查最近提交信息是否明确提到与该任务直接相关的未完成事项。
3. 按任务要求检查相关代码与测试，避免进行无关历史问题扫查。
4. 实现任务所需的最小正确改动。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，然后在需要时运行完整测试套件。
6. 若测试失败且失败未被明确排期，修复失败或在 `TODO.md` 中添加正确排序的前置任务。
7. 在 `TODO.md` 中将完成任务标题加 `[DONE]` 并更新完成记录。
8. 若阶段级计划实际变化才更新 `PLAN.md`。
9. 检查 `git status`、`git diff`、最近提交，提交本轮相关变更。
10. 停止，不处理下一项任务。

## 进度
- 已写入初始计划。
- 已读取 `TODO.md`，第一个未完成任务为 `P3.2 编辑 user 消息`。
- 下一步检查最近提交是否有与 P3.2 直接相关的未完成事项，然后聚焦 `crates/atto-ui-chat` 的 store/list/input/action 路径实现。
- 最近提交 `P3.1` 未提到 P3.2 相关未完成阻塞；当前仅 `memory/claude_plan.md` 变更。
- 已确认现有 `Edit` 按钮只触发 `MessageActionKind::EditUser`，尚未进入编辑态、回填输入、截断 store 或触发重发回调。
- 修订实现方向：`ChatMessageList::on_edit_and_resubmit(&ChatInputHandle, callback)` 注册编辑控制器与输入提交拦截器；点击 user `Edit` 时只进入 pending 编辑态并把原文写入输入 draft；输入提交编辑文本时由控制器调用 `truncate_from(message_id)` 并触发 `on_edit_and_resubmit`。
- 已完成核心代码改动：输入层新增文本提交拦截器；列表层新增 `EditAndResubmitEvent`、编辑控制器、`ChatMessageList::on_edit_and_resubmit`，并让配置专用编辑重发时的 user `Edit` 按钮走新路径。
- 已补单测覆盖 user 文本提取、输入回填、提交截断、事件 payload 与专用按钮行为。
- 验证进度：`cargo fmt --all`、`cargo test -p atto-ui-chat --lib`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets` 已通过；正在运行全量测试。
- 全量 `cargo test --all --all-targets` 已通过。
- `TODO.md` 已将 `P3.2 编辑 user 消息` 标记为 `[DONE]` 并补充完成记录。
- 下一步检查 diff/status/log，确认无无关变更后提交本轮修改。
- 已检查 `git diff --check` 通过；待提交文件仅包含 P3.2 相关代码、`TODO.md` 与本计划文件。
