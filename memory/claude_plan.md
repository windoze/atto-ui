# 当前执行计划

## 原则

- 以 `TODO.md` 为任务排序和完成状态来源；本轮只完成第一个标题未带 `[DONE]` 的任务。
- 本轮当前任务：`NT6 — TreeOp::InsertBefore 锚点版插入（R.1）`，详情见 `TODO-1.md`。
- 不用 workaround、夹具特化、缩窄范围或替代表达规避规格要求。
- 若发现阻塞 `NT6` 的真实缺陷或缺失能力，优先修复；若无法正确修复，则在 `TODO.md`/`TODO-1.md` 中插入最小必要前置任务并提交后停止。
- `PLAN.md` 只在阶段级顺序、依赖或完成标准变化时更新；当前仓库未发现根 `PLAN.md`。
- 不记录内部推理链；本文件记录可审计的任务目标、执行步骤、验证和进度。

## 任务要求摘要

- 在 `src/runtime/spec.rs` 的 `TreeOp` 中新增 `InsertBefore { parent_id, anchor_id, child }`。
- 在运行时树操作中支持：`anchor_id = None` 表示 append；指定 anchor 时按锚点节点位置插入；若 child id 已存在，语义等价于移动，即先从原父节点 detach 再插入。
- 在 `src/runtime/tree.rs` 的 `apply_ops_incremental` 中为 `InsertBefore` 增加增量分支，避免走全量重建。
- 旧 `Insert { index }` 和 Python 路径保持兼容。
- 测试覆盖 append、insert-before-anchor、已存在节点 move 三态，并验证增量路径不触发全量重建。

## 步骤

1. 查看最近提交信息，只判断是否有明确未完成且直接影响 `NT6` 的事项。
2. 读取 `src/runtime/spec.rs`、`src/runtime/tree.rs` 及现有 runtime/tree 测试，定位 `TreeOp::Insert`、`Move`、全量重建和增量分支实现。
3. 以最小改动新增 `TreeOp::InsertBefore`，保持 serde 形态与现有 `TreeOp` 兼容。
4. 实现运行时语义：append、按 anchor 插入、已存在 child 先 detach 再插入，并保留自身/子树移动保护。
5. 在增量路径加入 `InsertBefore` 处理，复用现有 insert/move 能力，确保不误触发全量重建。
6. 补充/更新单测，覆盖三态行为、非法 anchor/自身子树保护、增量路径不全量重建及旧 `Insert` 不回归。
7. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test`；必要时补跑更聚焦测试定位失败。
8. 更新 `TODO-1.md` 中 `NT6` 标题为 `[DONE]` 并填写完成记录，同步更新 `TODO.md` 索引状态。
9. 检查 `git status`、`git diff`、最近提交，暂存本轮相关改动并提交，然后停止。

## 进度

- 已读取 `TODO.md` 与 `TODO-1.md`，确认本轮第一个未完成任务为 `NT6 — TreeOp::InsertBefore 锚点版插入（R.1）`。
- 已确认根 `PLAN.md` 当前不存在；如阶段级计划未变化，本轮不更新计划文件。
- 已写入本轮执行计划，下一步查看最近提交并读取相关代码。
- 已查看最近提交，最新提交为 `[NR5] Review core native loading`，未发现明确声明 `NT6` 相关未完成事项。
- 已读取 `src/runtime/spec.rs`、`src/runtime/tree.rs`、`src/runtime/tests.rs`、Node 转换层与 `@atto-ui/core` 类型；确认需要同步更新核心 enum/语义、增量视图路径、Node JSON union 与 TS `TreeOp` 类型。
- 已完成初版实现：新增 `TreeOp::InsertBefore`，实现 append/anchor/direct-child 解析、已存在 id 的 detach-then-insert move 语义、自身/子树移动保护，并接入 `ComponentTree::apply_ops_incremental`、Node 转换层与 TS 类型。
- 已补充核心 spec、runtime 增量路径、Node conversion 与 TS 类型覆盖；下一步格式化并运行验证。
- 已运行 `cargo fmt`。
- 已运行并通过针对性测试：`cargo test -p atto-ui runtime::`、`cargo test -p atto-ui-node convert::tests::tree_op_parses_every_variant`。
- 已运行并通过完整/相关验证：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`、`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）、`npm test`（`crates/atto-ui-node`）、`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`、`npm test --prefix packages/core`。
- 已将 `TODO-1.md` 的 `NT6` 标记为 `[DONE]` 并补充完成记录；已同步更新 `TODO.md` 索引状态。
- 下一步检查 git 状态与 diff，暂存本轮相关改动并提交。
