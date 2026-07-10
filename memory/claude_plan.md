# 执行计划

## 约束
- 以 `TODO.md` 为唯一任务顺序来源，先完成第一个标题未带 `[DONE]` 的任务，然后停止。
- 不做开放式历史问题扫描；只处理当前任务直接相关或测试中暴露且未被排期的问题。
- 完成任务后更新 `TODO.md` 的标题和完成记录，必要时才更新 `PLAN.md`。
- 代码变更后依次运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试。
- 提交本轮所有相关未提交变更，提交后停止。

## 步骤
1. 读取 `TODO.md`，识别第一个标题未带 `[DONE]` 的任务及其验收要求。
2. 检查最新提交信息是否明确提到与该任务直接相关的未完成问题。
3. 阅读与当前任务相关的代码、测试和文档，确定最小正确实现范围。
4. 实现当前任务；如遇到阻塞性前置问题，按要求把最小前置任务插入 `TODO.md`、提交并停止。
5. 为实现补充或调整测试，避免用特例或规避路径代替规格要求。
6. 更新本计划文件记录关键进度或计划变更。
7. 运行格式化、lint 和相关/完整测试；任何未排期失败都修复或排期为前置任务。
8. 将当前任务在 `TODO.md` 标题前加 `[DONE]`，填写完成记录。
9. 检查 `git status`、`git diff` 和近期提交，提交本轮相关变更。

## 当前状态
- 已读取 `TODO.md`，第一个未完成任务为 `M4.R Review`。
- 本轮范围：复核 M4 skill 注入不会泄漏权限、不会破坏 prompt 预算，并完成要求的验证；若发现直接相关缺陷则修复后再完成 review。
- 最新提交未声明与 M4.R 直接相关的未完成事项。
- 已复核 skill prompt 预算与工具权限隔离路径：工具偏好只进入 prompt 元数据，执行权限仍由 `ToolRegistry` / `ToolPermissionPolicy` 决定。
- 已修复：`LoadedSkillSet` 改为去重但保留加载顺序，`build_skill_prompt_block` 因而按加载顺序注入 skill，保留后加载手动 skill 的优先级语义。
- 已新增并通过定向回归测试：`cargo test -p atto-agent-app skill_prompt_block_preserves_loaded_order_for_priority`。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已更新 `TODO.md`，将 `M4.R Review` 标记为 `[DONE]` 并写入完成记录。
- 下一步：检查 git 状态和 diff，确认只包含本轮相关变更后提交。
