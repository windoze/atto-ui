# 当前执行计划

## 范围

- 目标：根据 `TODO.md` 找到第一个未完成任务，完整完成该任务后停止。
- 约束：只完成一个任务；完成后更新 `TODO.md`，必要时更新 `PLAN.md`；运行规定验证；提交一次清晰的 Git commit。
- 说明：本文件记录可审阅的执行计划、关键决策和进度更新，不记录不可见的私有推理过程。

## 步骤

1. 读取 `TODO.md`，按文件顺序识别第一个标题未带 `[DONE]` 的任务。
2. 查看当前 Git 状态和最近提交，确认是否有与该任务直接相关的未完成内容或现有改动需要纳入本次任务。
3. 阅读该任务相关代码、测试和文档，确定最小正确实现范围。
4. 实现任务；如发现阻塞当前任务的真实缺口，优先修复，或在 `TODO.md` 中插入最小必要前置任务并停止。
5. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后运行相关测试；如代码有变更且需要完整验证，运行完整测试套件。
6. 更新 `TODO.md`：将完成任务标题加 `[DONE]`，填写完成记录和验证结果。仅在阶段计划确实变化时更新 `PLAN.md`。
7. 检查 `git status`、`git diff`、最近提交，确认只提交本任务相关文件。
8. 创建清晰的 Git commit，然后停止，不继续处理下一个任务。

## 当前状态

- 已读取 `TODO.md`，确认第一个未完成任务为 `T2 — macros trybuild 测试（A.1）`。
- 已检查 Git 状态和最近提交；现有未提交文档/脚本改动与 T2 无关，将避免纳入本次提交。
- 已阅读 `crates/atto-ui-macros` 的三个宏实现与现有 `tests/macro_view_builder.rs`。
- 已添加 `trybuild` harness、3 个成功展开 fixture、2 个失败诊断 fixture，并用 `TRYBUILD=overwrite cargo test -p atto-ui-macros` 生成 `.stderr`。
- 已通过验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-macros`；`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`：将 T2 标题标记为 `[DONE]`，并补充完成记录和验证结果。
- 下一步：检查 diff，确认不纳入无关工作区改动，然后提交 T2 相关变更。
