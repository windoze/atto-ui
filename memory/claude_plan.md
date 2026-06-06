# 当前执行计划

## 约束说明

- `TODO.md` 是任务顺序、要求与完成状态的权威来源。
- 本轮只完成第一个未标记 `[DONE]` 的任务，然后停止。
- 不做与当前任务无关的开放式历史问题扫查。
- 本文件记录可公开的执行计划和进度摘要，不记录私有推理细节。

## 执行步骤

1. 读取 `TODO.md`，确认首个未完成任务。
2. 检查最近提交是否包含与该任务直接相关的未完成事项。
3. 阅读任务涉及的代码和验证要求。
4. 以最小完整变更实现任务，不引入 workaround 或行为偏移。
5. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
6. 验证通过后，在 `TODO.md` 将当前任务标题标为 `[DONE]` 并补充完成记录。
7. 提交本任务相关变更，然后停止。

## 当前状态

- 已确认首个未完成任务为 `T14B — 拆分 editor app window 巨型文件（M8）`。
- 最新提交为 `[T14A] Split editor view module`，未提示 T14B 相关未完成问题。
- 已将 `crates/atto-editor-app/src/window.rs` 机械拆分为职责子模块：`component_impl.rs`、`tabs.rs`、`document_tab.rs`、`util.rs`。
- `window.rs` 现在只保留公开命令、句柄、主视图字段和构造入口。
- 拆分后 Clippy 仅发现多余 trait 导入，已移除。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。
- `TODO.md` 已将 T14B 标记为 `[DONE]` 并补充完成记录。
- 工作区存在与本任务无关的未跟踪文件 `notification.sh`、`run_agent.sh`，本次不修改、不提交。
- 下一步：提交 T14B 相关文件并停止。
