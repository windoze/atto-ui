# 当前执行计划

## 范围

- 目标：完成 `TODO.md` 中第一个未完成任务 `R2 — 审阅 T2`，然后停止。
- 约束：这是审阅任务，只审阅 T2 的宏 trybuild 测试质量与验证结果；不提前实现 T3。
- 说明：本文件记录可审阅的执行计划、关键决策和进度更新，不记录不可见的私有推理过程。

## 步骤

1. 查看 Git 状态和最近提交，确认是否有与 R2/T2 直接相关的未完成内容或现有改动需要纳入本次任务。
2. 阅读 `crates/atto-ui-macros` 的宏实现、T2 新增 trybuild harness、成功 fixture、失败 fixture 和 `.stderr`。
3. 对照 R2 验收项确认：三个宏核心展开路径是否真实覆盖；失败用例是否至少覆盖一类编译失败；错误信息是否为用户友好诊断而非裸 panic。
4. 如发现阻塞性缺陷，直接修复并更新测试；如发现必须另排前置任务，则更新 `TODO.md` 并停止。
5. 按要求运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-macros`；若相关代码改变并需要完整回归，再运行更大范围测试。
6. 更新 `TODO.md`：将 `R2` 标题加 `[DONE]`，补充审阅结论和验证记录。仅在阶段计划变化时更新 `PLAN.md`。
7. 检查 `git status`、`git diff`、最近提交，确认只提交 R2 相关文件。
8. 创建清晰的 Git commit，然后停止，不继续处理 `T3`。

## 当前状态

- 已读取 `TODO.md`，确认第一个未完成任务为 `R2 — 审阅 T2`。
- 已检查 Git 状态和最近提交：工作区存在非本次文档/脚本改动，将避免纳入 R2 提交；最新提交是 T2 完成记录，无需新增前置任务。
- 已审阅 T2 的 `trybuild` harness、3 个成功 fixture、2 个失败 fixture 和 `.stderr`：覆盖 `Reactive`、`view_builder!`、`ComponentProperties`/`component_properties` 核心路径，失败诊断为明确的 `compile_error!` 文案。
- 已通过验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-macros`。
- 已更新 `TODO.md`：将 `R2` 标题标记为 `[DONE]`，并补充审阅结论和验证记录。
- 下一步检查本次 diff，提交 R2 审阅记录后停止。
