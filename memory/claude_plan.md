# 执行计划

## 当前目标

- 按 `TODO.md` 的顺序识别第一个标题未带 `[DONE]` 的任务。
- 只完成这个第一个未完成任务，验证后更新任务记录并提交，然后停止。

## 约束

- `TODO.md` 是任务状态、顺序、依赖和完成记录的权威来源。
- `PLAN.md` 只在阶段级计划、依赖或完成标准变化时更新。
- 若发现会阻塞当前任务的真实缺陷或未排期失败测试，必须修复，或在 `TODO.md` 中加入最小必要前置任务并提交后停止。
- 不接受缩小范围、绕过规格或任务私有特判作为完成方式。
- 编辑前先说明将修改的内容；编辑时使用小而集中的补丁。
- 验证顺序为 `cargo fmt`，`cargo clippy --all-targets -- -D warnings`，然后完整测试；若只有文档变化且已有可复用的绿色完整测试结果，可在完成记录中说明跳过。

## 步骤计划

1. 读取 `TODO.md`，确定第一个未完成任务及其验收要求。
2. 查看最新提交信息，判断是否提到与该任务直接相关的未完成问题。
3. 根据任务内容只读取必要代码和测试上下文。
4. 实现当前任务，若发现阻塞性前置问题则按要求更新 `TODO.md` 并停止。
5. 运行格式化、lint 和相关/完整测试，修复所有未排期失败。
6. 在 `TODO.md` 中将任务标题加上 `[DONE]`，补全完成记录；仅在阶段计划变化时更新 `PLAN.md`。
7. 检查工作区变更并提交一次清晰的 Git commit。
8. 停止，不处理下一个任务。

## 进度记录

- 2026-07-15：已写入初始执行计划，下一步读取 `TODO.md` 和最新提交信息。
- 2026-07-15：已确定第一个未完成任务为 `M2-1 Checkbox apply_command`。最新提交 `[M1-R] Review layer 1 introspection foundation` 未显示与该任务直接相关的未完成问题。接下来读取 `src/widgets/checkbox.rs`、`src/component_api.rs`、`src/composable/component.rs` 及现有 checkbox 测试，目标是复用既有 checkbox 状态切换/回调路径实现 `Toggle` 与 `Click`，再补进程内测试并运行规定验证。
- 2026-07-15：已在 `Checkbox::apply_command` 中支持 `ComponentCommand::Toggle` 与 `ComponentCommand::Click`，两者均检查 `enabled` 后复用现有 `toggle()` 路径；禁用态和其他命令返回 `EventResult::ignored()`。已新增进程内单测覆盖 binding 翻转、click change callback payload、禁用态 ignored。
- 2026-07-15：验证已完成并通过：`cargo test -p atto-ui checkbox -- --nocapture`、`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。下一步更新 `TODO.md` 的 M2-1 完成记录并提交。
- 2026-07-15：已将 `TODO.md` 中 M2-1 标记为 `[DONE]` 并补完成记录/验证记录。完整测试后仅修改 `TODO.md` 与本计划文件，因此不重新运行测试。已检查 diff/status 并提交为 `[M2-1] Implement checkbox apply command`。
