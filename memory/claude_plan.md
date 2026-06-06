# 当前调用计划

本文件记录本次调用的可执行计划与进度摘要。内容只包含可检查的计划、决策和结果，不记录私有推理过程。

## 初始计划

1. 先读取 `TODO.md`，找出标题未以 `[DONE]` 开头的第一个任务。
2. 只检查最新提交中与所选任务直接相关的未完成事项。
3. 阅读所选任务的要求、依赖和验证说明。
4. 完整实现所选任务；如果遇到阻塞正确实现的具体前置问题，则在 `TODO.md` 中加入最小前置任务并停止。
5. 按要求依次运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --workspace --all-targets` 和 `cargo test --all --all-targets`。
6. 将完成的任务标题标记为 `[DONE]` 并写入完成记录；仅在阶段级计划变化时更新 `PLAN.md`。
7. 关键步骤完成或计划变化时更新本文件。
8. 检查 git 状态、差异和近期提交，提交本轮相关变更，然后停止，不进入下一任务。

## 进度

- 已从 `TODO.md` 选定第一个未完成任务：`T13 — 命名消歧义（按 T13A 确认结果执行）`。
- T13 要求将 `atto-editor` 改名为 `atto-editor-app`，并将原独立 runtime crate 合并进 `atto-ui`，随后验证全 workspace。
- 最新提交仍记录旧的 T13A 确认阻塞，但当前工作区已有相关 `TODO.md`/`PLAN.md` 变更，将 T13A 标为已确认；这些变更是 T13 的相关前置状态，会保留。
- 已将 editor app 目录改为 `crates/atto-editor-app`，更新 package 名、import、测试和文档，并把原独立 runtime 源码移到 `src/runtime/spec.rs`，通过 `atto_ui::runtime` 导出。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- `TODO.md` 已将 T13 标记为 `[DONE]` 并写入完成记录。

## T13 执行步骤

1. 检查 workspace manifest、crate 布局，以及所有 `atto-editor`、`atto_editor`、原独立 runtime crate 名和旧 runtime import 引用。
2. 将 `crates/atto-editor` 改名为 `crates/atto-editor-app`，更新 package 名、workspace members、依赖、import、lockfile、文档和脚本/CI 引用。
3. 将原独立 runtime crate 合并进根 `atto-ui` crate，放到 `src/runtime/spec.rs`，由 `atto-ui` 的 `runtime` 模块导出，并将旧外部 crate import 改为 `atto_ui::runtime`。
4. 移除独立 runtime workspace crate 及相关 dependency 条目。
5. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`，并修复所有问题。
6. 在 `TODO.md` 中将 T13 标记为 `[DONE]` 并添加完成记录，提交本轮相关变更后停止。
