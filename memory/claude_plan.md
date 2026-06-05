## 执行计划

1. 读取 `TODO.md`，按文件顺序识别第一个标题未带 `[DONE]` 的任务。
2. 检查该任务的要求、依赖、验证方式和完成记录；仅在与当前任务直接相关时查看最近提交或相关代码。
3. 实现当前任务，遇到阻塞时只添加最小必要的前置任务到 `TODO.md` 并停止。
4. 按要求运行格式化、lint 和相关/完整测试；发现未被明确排期的失败时立即修复或排期。
5. 更新 `TODO.md`，将完成的任务标题加上 `[DONE]` 并填写完成记录；仅在阶段计划改变时更新 `PLAN.md`。
6. 检查 Git 状态和差异，提交本次任务涉及的全部变更，然后停止，不处理下一个任务。

## 当前进度

- 已读取 `TODO.md`，确认第一个未完成任务为 `R9 — 审阅 T9`。
- 当前只审阅 T9 的共享滚动逻辑抽取，不处理后续 `T10`。
- 已审阅 `src/composable/scroll.rs`、stack/grid/scroll_container 事件路径与 `src/wm/min_size_view.rs`，未发现行为偏差。
- 已验证 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关滚动测试与全量 `cargo test --all --all-targets` 全部通过。
- 已更新 `TODO.md`，将 `R9` 标记为 `[DONE]` 并写入完成记录。
- 下一步检查差异并提交 R9 变更。
