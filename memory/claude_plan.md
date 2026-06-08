# 执行计划

## 范围

- 以 `TODO.md` 为任务顺序和完成状态的权威来源。
- 本轮只完成第一个未标记 `[DONE]` 的任务，然后停止。
- 若遇到阻塞当前任务的真实前置问题，则只更新任务列表、记录阻塞、提交并停止。

## 步骤

1. 读取 `TODO.md`，确认第一个未完成任务及其来源文件。
2. 读取 `TODO-2.md` 中该任务的完整要求、依赖、验收和完成记录。
3. 只围绕当前任务检查相关提交、源码、测试和文档，不做无关历史问题扫描。
4. 如发现阻塞当前任务的缺陷或缺失前置项，按最小范围更新 `TODO.md` / `TODO-2.md` 并停止。
5. 对当前任务执行必要实现或审阅；本轮任务为 `R15`，因此重点审阅 `T15` 的实现路径。
6. 按顺序运行格式化、lint 和测试验证：先 `cargo fmt`，再 clippy，最后测试。
7. 将当前任务标题标记为 `[DONE]`，补充完成记录，并同步根 `TODO.md` 索引。
8. 检查 `git status`、`git diff` 和最近提交，只暂存本轮 intended files。
9. 用任务范围内的清晰提交信息提交，然后停止，不进入下一个任务。

## 进度记录

- 已在任务检查前初始化本执行计划。
- 已确认第一个未完成任务为 `R15 — 审阅 T15`。
- 审阅范围：File Picker 的文件/目录过滤和 `.git` 隐藏、workspace root 变化后的 index invalidation、Buffer Picker stable tab id accept 路径、File Picker accept 复用 `open_path` 且不重复添加 workspace root。
- 已完成实现审阅，未发现需要修改 `T15` 功能代码的问题。
- 验证已通过：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo clippy --workspace --all-targets -- -D warnings`，以及 workspace clippy 通过后的最终 `cargo test --all --all-targets`。
- 已在 `TODO-2.md` 和根 `TODO.md` 索引中将 `R15` 标记为 `[DONE]`。
