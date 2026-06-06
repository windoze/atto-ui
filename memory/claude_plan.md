# 执行计划

## 目标

- 严格以 `TODO.md` 为任务来源，完成第一个标题未带 `[DONE]` 的任务，然后停止。
- 若遇到阻塞当前任务的既有缺陷、测试失败或规格不匹配，先修复或在 `TODO.md` 中插入最小必要前置任务并提交。
- 完成后更新 `TODO.md` 的任务标题与完成记录，按要求格式化、检查、测试，并提交一次清晰的 Git commit。

## 步骤

1. 读取 `TODO.md`，只定位第一个未完成任务，不做开放式历史问题排查。
2. 查看最新提交与当前工作区状态，判断是否存在与该任务直接相关的未完成事项或未提交恢复状态。
3. 阅读当前任务涉及的代码、测试和文档，确认验收要求与依赖。
4. 若任务可直接完成，实施最小正确改动；若被具体缺口阻塞，按要求更新 `TODO.md` 记录前置任务并停止。
5. 为实现补充或调整相关测试，避免规避规格或只为夹具通过而特判。
6. 运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过后运行完整测试套件 `cargo test --workspace --all-targets`，除非本轮仅改文档且可复用上次绿色结果。
7. 将任务标题加 `[DONE]` 并更新 completion record；仅在阶段计划真实变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、`git log --oneline -10`，只提交本轮相关文件；若是恢复未完成任务，则按要求包含当前未提交文件。
9. 提交后停止，不进入下一个任务。

## 进度记录

- 已创建本计划文件，下一步读取 `TODO.md` 识别第一个未完成任务。
- 已读取 `TODO.md`，第一个未完成任务为 `R8 — 审阅 T8`。
- R8 执行范围：审阅 `ChatMessageStore::append_delta`、通知粒度相关实现和 chat 测试；如发现阻塞问题则修复，否则仅补充完成记录与验证结果。
- 审阅发现 `set_status` 重复设置相同状态会产生无效 dirty，已计划按 R8 通知粒度要求做最小修复并补充回归测试。
- 已修复 `set_status` 同状态 no-op 不通知，并新增 `update_text` 同文本 no-op、`set_status` 同状态 no-op、`ForEachIdentifiable` 多项列表仅重建变更项的测试。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat`、`cargo test -p atto-ui foreach_id_rebuilds_only_changed_items`、`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 `R8` 标记为 `[DONE]` 并写入完成记录；下一步检查 diff/status/log 后提交本轮相关文件。
