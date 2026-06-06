# 执行计划

## 当前约束

- 以 `TODO.md` 为唯一任务顺序与完成状态来源，先找出第一个标题未带 `[DONE]` 的任务。
- 本轮只完成第一个未完成任务；完成后更新 `TODO.md`、验证、提交，然后停止。
- 若遇到阻塞当前任务的真实前置问题，不绕过；将最小必要前置任务写入 `TODO.md`，提交后停止。
- `PLAN.md` 仅在阶段级计划、依赖或完成标准变化时更新。
- 代码变更后按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试。

## 初始执行步骤

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务。
2. 查看最新提交信息，判断是否有与该任务直接相关的未完成事项。
3. 阅读该任务涉及的代码、测试和文档，确定最小正确实现范围。
4. 实施任务或在发现不可绕过的阻塞时更新 `TODO.md` 记录前置任务。
5. 运行格式化、lint 和相关/完整测试，修复所有未计划的失败。
6. 更新 `TODO.md` 的任务标题与完成记录；必要时更新本文件记录关键进展。
7. 检查 `git status`、`git diff`、近期提交，仅提交本轮相关改动。

## 进度记录

- 已创建本计划文件，下一步读取 `TODO.md` 确认本轮任务。
- 已确认第一个未完成任务为 `T8 — ChatMessageStore 增量流式（C.1）`。
- 最新提交为 `[R7] Review async runtime crate`，未直接指出 T8 相关未完成事项。
- 当前执行范围：检查 `crates/atto-ui-chat/src/store.rs` 与 `message.rs`，实现 `append_delta(id, &str)`，覆盖文本增量追加、非文本安全 no-op、InProgress/Final 流式状态语义，以及长文本追加测试。
- 已实现核心改动：`Property` / `Binding` 增加 `update_if`，`ChatMessageStore::append_delta` 使用原地追加；`update_text` 不再对非文本或相同文本产生脏通知；chat demo 改为传增量 delta，不再每步构造累计全文。
- 已完成验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- 已将 `TODO.md` 中 T8 标记为 `[DONE]` 并补充完成记录。下一步检查 diff/status 后提交本轮改动。
