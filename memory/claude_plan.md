# 执行计划

## 当前目标

以 `TODO.md` 为权威任务列表，只完成第一个标题未带 `[DONE]` 的任务，完成验证、任务记录更新和 Git 提交后停止。

## 执行步骤

1. 读取 `TODO.md`，先确认第一个未完成任务，不做开放式历史问题扫查。
2. 仅检查最近提交信息是否提到与该任务直接相关的未完成事项。
3. 阅读该任务在来源文件中的详细要求、依赖、验收项和相关实现文件。
4. 按任务要求完整实现或审阅，使用小而聚焦的补丁，不通过缩小范围或绕过问题推进。
5. 如发现阻塞当前任务的具体前置问题，向 `TODO.md` 的正确依赖位置加入最小必要前置任务，保持当前任务未完成，提交后停止。
6. 按要求运行验证：先 `cargo fmt`，再 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行相关或完整测试。
7. 对发现的未排期测试/fixture 失败进行修复，或按依赖顺序显式排入 `TODO.md`。
8. 完成后在 `TODO.md` 和来源任务文件中给任务标题加 `[DONE]`，并更新完成记录。
9. 仅当阶段级顺序、依赖、假设或完成标准变化时更新 `PLAN.md`。
10. 提交前检查 `git status`、`git diff` 和最近提交，确认只提交本轮相关文件。
11. 使用清晰提交信息提交本轮变更，然后停止，不进入下一项任务。

## 进度记录

- 已在运行项目检查或实现命令前初始化本执行计划。
- 已确认第一个未完成任务是 `R8 — 审阅 T8`，来源 `TODO-2.md`；最近提交信息未指出与 R8 直接相关的未完成阻塞项。
- 已审阅 T8 相关实现面：editor diagnostics gutter/layout、F8/Shift+F8 跳转、diagnostics theme 映射、document-tab summary 传播和 app statusbar fallback。
- 已补充一个聚焦回归测试，固定 wrapped diagnostics continuation row 只保留 gutter 空白，不重复显示 marker，也不覆盖文本起始列。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
- 已在 `TODO.md` 和 `TODO-2.md` 将 `R8` 标记为 `[DONE]`，并写入审阅结论与验证结果。
