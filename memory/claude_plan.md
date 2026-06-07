# 执行计划

## 约束

- 以 `TODO.md` 为唯一任务顺序与完成状态来源。
- 本次只完成第一个标题未标记 `[DONE]` 的任务，然后停止。
- 不做开放式历史问题扫描；仅处理阻塞当前任务或验证中暴露且未被调度的失败。
- 若无法按原任务完成，更新 `TODO.md` 插入最小必要前置任务，提交后停止。
- 任务完成后运行格式化、Clippy、相关测试；若适用再运行完整测试。
- 任务完成必须在 `TODO.md` 标题加 `[DONE]` 并更新完成记录。
- 最终提交本次任务相关变更。

## 初始步骤

1. 读取 `TODO.md`，识别第一个标题未带 `[DONE]` 的任务。
2. 查看最新提交信息，仅判断是否有直接关联当前任务的未完成事项。
3. 读取当前任务相关源码、测试与文档，确定最小实现范围。
4. 实现任务，不规避规格要求或测试失败。
5. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`。
6. 运行当前任务相关测试；必要时运行完整测试套件。
7. 更新 `TODO.md` 的任务标题与完成记录；仅在阶段计划变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交，提交本次任务相关文件。
9. 停止，不继续下一个任务。

## 进度记录

- 已创建初始执行计划，下一步读取 `TODO.md`。
- 已读取 `TODO.md` 与 `TODO-1.md`，第一个未完成任务为 `NR12 — 审阅 NT12`。
- `NR12` 审阅范围：确认 React 文本组件只产出 `TextSpan`、片段合并在 Rust `RichText`；确认 Link payload 到 `onClick` 路由；确认 `Markdown` 到 `MarkdownViewer.markdown` 映射；运行快照与 PTY 相关验证。
- 已检查最新提交 `95e5e5b [NT12] Add React text components`，未发现提交信息中声明的未完成事项。
- 已审阅 `packages/react/src/text.ts`、`host.ts`、事件分发和相关 JS/PTY 测试；当前未发现需要先修复的阻塞问题，下一步运行规定验证。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、core/react TypeScript typecheck、`cargo test --all --all-targets`、Node native build、Node/core/react JS 测试、`git diff --check`。
- 已将 `TODO.md` 索引与 `TODO-1.md` 中 `NR12` 标记为 `[DONE]` 并填写完成记录；下一步检查 diff 并提交。
