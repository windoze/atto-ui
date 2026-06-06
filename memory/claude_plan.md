# 当前调用计划

本文件记录本次调用的可执行计划与进度摘要。内容只包含可检查的计划、决策和结果，不记录私有推理过程。

## 初始计划

1. 先读取 `TODO.md`，找出标题未以 `[DONE]` 开头的第一个任务。
2. 阅读所选任务的要求、依赖、验证说明和最新相关提交。
3. 只检查与当前任务直接相关的代码、测试、Cargo 配置和文档，避免无关历史问题扫查。
4. 完整执行当前任务；如果发现阻塞正确完成的具体问题，则修复该问题或在 `TODO.md` 中添加最小前置任务后停止。
5. 按要求依次运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、必要的构建验证和全量测试。
6. 将完成的任务标题标记为 `[DONE]` 并写入完成记录；仅在阶段级计划变化时更新 `PLAN.md`。
7. 关键步骤完成或计划变化时更新本文件。
8. 检查 git 状态、差异和近期提交，提交本轮相关变更，然后停止，不进入下一任务。

## 进度

- 已初始化本次计划。
- 已从 `TODO.md` 选定第一个未完成任务：`R13 — 审阅 T13`。
- R13 范围：审阅 T13 的 editor app 改名和 runtime 合并结果，确认全工作区编译、旧引用、CI/文档同步状态，修复直接相关遗漏后验证并提交。
- 审阅发现 runtime 合并后本地仍有空的 `crates/atto-ui-runtime/` 目录；该目录无 tracked 文件，已从工作区移除。
- 审阅发现 `CLAUDE.md` 未随 T13 同步；已补充当前 workspace crate 清单、`atto-ui::runtime` 说明、`atto-editor-app` 应用 crate 以及 Python/组件聚合入口说明。
- 已完成 `cargo fmt`。
- 已完成 `cargo clippy --workspace --all-targets -- -D warnings`。
- 已完成 `cargo build --workspace --all-targets`。
- 已完成 `cargo test --all --all-targets`。
- 已在 `TODO.md` 将 `R13 — 审阅 T13` 标记为 `[DONE]`，并记录审阅结果、文档修复、旧引用搜索、CI 复核和验证命令。
