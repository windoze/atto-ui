# 执行计划

## 说明

本文件记录本次执行的可审查思路摘要、任务计划、关键进展和验证结果。不会记录不可公开的逐字内部推理；会记录足以复核决策的依据、约束和执行步骤。

## 初始思路摘要

- `TODO.md` 是任务顺序和完成状态的唯一权威来源。
- 只处理第一个标题未带 `[DONE]` 的任务；完成后必须停止，不继续下一个任务。
- `PLAN.md` 只在阶段级计划、依赖或完成标准变化时更新。
- 如果当前任务遇到阻塞或测试失败，不能绕过；必须修复，或在 `TODO.md` 中加入最小必要前置任务并提交后停止。
- 完成任务后需要更新 `TODO.md` 的标题和完成记录，运行格式化、lint 和必要测试，然后提交 Git。

## 步骤计划

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务，并记录任务编号、范围和验证要求。
2. 查看最新提交信息；如果它明确提到与当前任务直接相关的未完成问题，将其纳入当前任务或作为前置任务写入 `TODO.md`。
3. 根据当前任务范围读取相关源文件、测试和文档，避免无边界历史问题排查。
4. 如任务可直接完成，按现有项目风格实现；如发现必要前置阻塞，最小化修改 `TODO.md` 并停止。
5. 更新或新增聚焦测试，覆盖当前任务要求和相关边界。
6. 运行 `cargo fmt`。
7. 运行 `cargo clippy --all-targets -- -D warnings`。
8. 运行完整测试套件，优先使用 `cargo test --all --all-targets`，并确保超时不超过 30 分钟。
9. 若验证失败，修复失败原因；未被后续任务明确排期的失败不能忽略。
10. 更新 `TODO.md`：给当前任务标题添加 `[DONE]`，补充完成记录和验证结果。仅在阶段计划变化时更新 `PLAN.md`。
11. 检查 Git 工作区，确认包含本次任务相关改动；如是恢复上次未完成任务，则把当前未提交文件一并纳入同一提交。
12. 使用清晰的任务编号提交信息提交变更。
13. 停止，不处理后续任务。

## 进展记录

- 已创建初始执行计划，下一步读取 `TODO.md`。
- 已读取 `TODO.md`，确认第一个未完成任务是 `M1-2 DesktopInspector 收敛为第 1 层门面`。
- 已查看最新提交：`99b29e75 [M1-1] Add public find_by_tag API`。该提交是当前任务的直接前置基础，未发现提交信息中声明的相关未完成阻塞。

## 当前任务计划：M1-2

1. 已阅读 `src/inspect.rs`、`src/composable/find.rs`、`src/component_api.rs` 和相关测试，确认现有 `DesktopInspector` 的三类寻址路径。
2. 已新增 `DesktopInspector::property_names(id) -> Result<Vec<String>, ComponentError>`，按 menu、window、component 三段式查找，错误风格对齐 `get_property`。
3. 已确认 `get_property` / `set_property` 的组件寻址通过 `component_find` / `component_find_mut` 委托 M1-1 公共 `find_by_tag` / `find_by_tag_mut`；`export_snapshot` 是全树导出，不按组件 id 寻址。
4. 已新增单测覆盖组件属性名集合、menu/window 路径，以及未知 tag 返回 `ComponentError::NotFound`。
5. 已运行聚焦测试 `cargo test -p atto-ui inspect_property_names -- --nocapture`，测试通过；已修复过程中发现的未使用变量警告。
6. 已运行 `cargo fmt --all` 和 `cargo fmt --all -- --check`，格式检查通过。
7. 已运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过且无警告。
8. 已运行带 30 分钟上限的完整测试套件：`cargo test --workspace --all-targets`，通过。
9. 已更新 `TODO.md`：M1-2 标题标记为 `[DONE]`，并补充完成记录与验证命令。继续做最终 diff / status 检查并提交。

## 验证记录

- `cargo test -p atto-ui inspect_property_names -- --nocapture`：通过。
- `cargo fmt --all`：通过。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`：通过。
