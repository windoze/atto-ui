# 当前执行计划

## 范围

- 以 `TODO.md` 为唯一任务顺序和完成状态来源。
- 本次只处理第一个标题未带 `[DONE]` 的任务，完成后提交并停止。
- 若遇到阻塞当前任务的缺陷、规格不匹配或测试失败，优先修复；若无法在本次完成，则在 `TODO.md` 中加入最小必要前置任务并提交后停止。

## 步骤

1. 读取 `TODO.md`，定位第一个未完成任务及其依赖、验证要求和完成记录要求。
2. 检查最近提交信息是否明确提到与该任务直接相关的未完成问题。
3. 读取与当前任务相关的代码、测试和文档，避免无关历史排查。
4. 实现当前任务要求，保持改动尽量小且符合现有代码结构。
5. 按要求运行格式化、lint 和相关测试；若代码有变更，优先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后运行必要的完整测试。
6. 更新 `TODO.md`：在任务标题前加 `[DONE]`，填写完成记录、验证结果和必要说明。
7. 仅当阶段计划或依赖结构变化时更新 `PLAN.md`。
8. 检查工作区差异，提交本次任务涉及的所有未提交文件。
9. 停止，不继续处理下一个任务。

## 进度

- 已写入初始执行计划。
- 已读取 `TODO.md`，本次任务确定为 `R6 — 审阅 T6`。
- 最近提交为 `[T6] Add task cancellation registry`，与本次审阅任务直接相关，将纳入审阅范围。
- 审阅发现 `TaskRegistry` 的 `running` 属性更新与任务列表变更不在同一临界区，存在并发注册/注销时运行态短暂或最终不一致的风险；将先修复该问题，再继续验证 R6。
- 已修复运行态更新临界区问题，并新增运行态边界通知单测。
- 已完成验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui task::`、`cargo test -p atto-ui app::run::tests::apphost_escape`、`cargo test --test pty_async_actions`、`cargo tree -p atto-ui`、`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 `R6 — 审阅 T6` 标记为 `[DONE]` 并写入完成记录。
