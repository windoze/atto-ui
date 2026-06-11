执行计划记录

范围说明：本文件记录可检查的执行计划、关键决策和进度；不包含隐藏推理过程。

初始计划：
1. 读取 TODO.md，按标题是否带有 [DONE] 判断第一个未完成任务。
2. 阅读该任务的上下文、依赖、验证要求，以及必要的 PLAN.md 相关阶段信息。
3. 检查最近提交是否明确提到与该任务直接相关的未完成问题；只处理会阻塞当前任务的问题。
4. 实现当前任务要求，保持改动最小且符合项目风格。
5. 按要求先运行 cargo fmt，再运行 cargo clippy --all-targets -- -D warnings，最后运行完整测试套件；如仅文档变更且满足条件则记录跳过依据。
6. 若发现未安排的测试或夹具失败，优先修复或在 TODO.md 中加入最小必要前置任务并停止。
7. 完成后在 TODO.md 中给当前任务标题加 [DONE]，更新完成记录；仅在阶段计划改变时更新 PLAN.md。
8. 检查 git 状态和差异，提交本次任务相关改动，然后停止，不继续下一个任务。

当前状态：已读取 TODO.md，第一个未完成任务为 `P8.3 聊天文本选择`。

P8.3 执行计划：
1. 已阅读 `CHAT_UI.md` / `PLAN.md` 中 P8 文本选择要求，并检查 `src/list.rs`、`snapshot_chat_app.rs`、PTY 测试中已有 CopyBlock 实现。
2. 已检查最近提交；最新提交为 P8.2，不包含直接阻塞 P8.3 的未完成问题。
3. 已在现有 `BlockCopyTarget` 包装层加入基于渲染行的选择状态、鼠标拖选、选区渲染和“有选择时复制所选文本、无选择时仍触发 CopyBlock”的行为，避免改动公开 `MessageActionKind` API。
4. 已补充单元测试覆盖跨视觉行/宽字符选择和拖选高亮。
5. 已增加 `snapshot_chat_app --text-selection` 场景，并新增 PTY 覆盖高亮、OSC52 复制所选文本、无选区 fallback。
6. 已运行 `cargo fmt --all`；修复 clippy 指出的 `needless_range_loop` 后，`cargo clippy --workspace --all-targets -- -D warnings` 通过。新增定向验证已通过：`cargo test -p atto-ui-chat selection`、`cargo test -p atto-ui-chat --test pty_chat chat_text_selection_highlights_copies_selection_and_falls_back -- --exact`。完整验证已通过：`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。仓库未发现 `tools/run_fixtures.py` 夹具入口。
7. 已更新 TODO.md，将 P8.3 标记为 `[DONE]` 并记录完成与验证结果。
8. 已检查 git 状态、差异和最近提交；已暂存本次任务相关文件，下一步提交本次任务改动后停止。
