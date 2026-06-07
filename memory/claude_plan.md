# 执行计划

## 范围

- 以 `TODO.md` 为唯一任务顺序和完成状态来源。
- 本次只完成第一个标题尚未带 `[DONE]` 的任务，然后停止。
- 若遇到阻塞当前任务的缺陷、规格不匹配或缺失能力，先修复；若无法在本次按规格修复，则在 `TODO.md` 中添加最小必要前置任务并提交后停止。

## 步骤

1. 读取 `TODO.md`，定位第一个未完成任务。
2. 查看与该任务直接相关的上下文文件；如最新提交明确提到该任务的未完成事项，也纳入当前范围。
3. 按任务要求做最小正确实现，避免无关重构或绕过规格。
4. 运行格式化、lint 和相关测试；若代码有实质变更，按要求执行完整验证。
5. 更新 `TODO.md`：将完成任务标题加 `[DONE]`，填写完成记录和验证结果；仅在阶段计划变化时更新 `PLAN.md`。
6. 检查 `git status`、`git diff` 和近期提交，确认只提交本次相关改动。
7. 使用清晰的任务编号提交信息创建 Git commit。
8. 停止，不继续下一个任务。

## 进度日志

- 已创建初始执行计划；下一步读取 `TODO.md` 识别首个未完成任务。
- 已读取 `TODO.md`；首个未完成任务为 `NT2`（`serde 数据转换层（B.2）`），详情来源 `TODO-1.md`。
- 已确认最新提交为 `[NR1] Review Node napi scaffold`，未发现直接声明 `NT2` 未完成事项。
- 已读取 `TODO-1.md`、`PLAN-1.md`、`NODE_BINDING.md` 和现有 Node/Python binding；`NT2` 的实现范围为 `crates/atto-ui-node/src/convert.rs`，使用 napi `serde-json` 把 JS 值桥接为 `serde_json::Value`，再转换为 runtime 类型。
- 下一步实施：新增 `convert.rs`，覆盖 `ComponentValue`、`ComponentSpec`/child/layout、`TreeOp`、`CallbackInvocation` 和 `ComponentSchema` 的双向转换与单测；随后执行格式化、clippy、测试和任务记录更新。
- 已实现 `convert.rs` 并接入 Node crate；新增单测覆盖 `ComponentValue` 全主要分支、`ComponentSpec`/layout/meta、全部现有 `TreeOp` 变体、callback invocation、schema round-trip 和错误上下文。
- 验证通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-node`、`cargo test --all --all-targets`。
- 已更新 `TODO.md` 和 `TODO-1.md`，将 `NT2` 标记为 `[DONE]`/`DONE` 并写入完成记录。下一步检查 git diff/status 后提交本任务。
