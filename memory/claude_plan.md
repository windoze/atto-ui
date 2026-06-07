# 执行计划

## 约束确认

- 以 `TODO.md` 为任务顺序和完成状态的唯一来源。
- 只完成第一个未在标题中标记 `[DONE]` 的任务，然后停止。
- 不做开放式历史问题扫查；只处理当前任务直接相关或验证中暴露且未排期的失败。
- 完成后需要更新 `TODO.md` 的标题和完成记录，必要时才更新 `PLAN.md`。
- 代码或任务变更完成后运行格式化、lint 和相关测试；如需完整测试，按要求设置足够超时。
- 最终提交 Git commit，不继续下一个任务。

## 步骤计划

1. 读取 `TODO.md`，确定第一个标题未带 `[DONE]` 的任务。
2. 检查最新提交信息，只有当它明确提到与该任务直接相关的未完成问题时，才纳入当前任务或作为前置项记录。
3. 阅读当前任务涉及的代码、测试和文档，确认需求、依赖和验证要求。
4. 若任务被具体缺陷或缺失能力阻塞，则在 `TODO.md` 中加入最小必要前置任务，更新当前任务依赖说明，提交后停止。
5. 若可直接完成，则进行最小正确实现，避免无关重构和变通方案。
6. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，然后按任务要求运行相关或完整测试。
7. 若验证发现未排期失败，修复失败或把最小必要修复任务加入 `TODO.md`，不得把当前任务标为完成后遗留未排期失败。
8. 验证通过后，在 `TODO.md` 中给当前任务标题加 `[DONE]` 并填写完成记录；仅在阶段计划真实变化时更新 `PLAN.md`。
9. 检查 `git status`、`git diff` 和最近提交，确认只提交本次相关变更。
10. 使用清晰提交信息提交本次任务变更，然后停止。

## 进度记录

- 已创建本计划文件；下一步读取 `TODO.md` 并定位第一个未完成任务。
- 已读取 `TODO.md`；第一个未完成任务为 `NT12`（React 文本组件，来源 `TODO-1.md` 阶段四 M5）。下一步读取 `TODO-1.md` 中 `NT12` 的完整要求，并检查最新提交是否包含与 `NT12` 直接相关的未完成事项。
- 已读取 `TODO-1.md` 中 `NT12` 完整要求：实现 `packages/react/src/text.ts`，覆盖 `createTextInstance`/`commitTextUpdate`、`<Text>` 与内联样式组件、`<Link href onClick>` 事件、`<Markdown>` 到 `MarkdownViewer` 的映射，并补充快照与 PTY 链接点击测试。
- 已检查最新提交 `2e9ac36 [NR11] Record final review status`，提交信息未声明与 `NT12` 直接相关的未完成事项。
- 当前执行计划调整为：梳理现有 React HostConfig、Node binding TreeOp/RichText 结构、MarkdownViewer 暴露方式和测试入口；随后最小实现文本 host 类型与事件路由，补齐测试并验证。
- 已确认 runtime 侧已有 `RichText`/`TextSpan` 与 `MarkdownViewer` 注册能力；当前实现策略为新增 `packages/react/src/text.ts`，让 React 文本组件生成 `RichText` + 多个 `TextSpan`，样式 flags 写入 `TextSpan` props，相邻片段仍交由 Rust `RichText` 归一化合并。链接事件在 `RichText` 上绑定 `link`，根据 payload URL 路由到对应 `Link.onClick`。
- 需要同步修改 `packages/react/src/host.ts` 增加 `markdownViewer` 类型映射，修改 `packages/react/src/index.ts` 导出文本组件，并补充 mock snapshot 与 PTY 链接点击测试。
- 已完成实现与首轮验证：新增 React 文本组件、导出 API、补充 `MarkdownViewer` host 映射、补充 reconciler/headless/PTY 文本测试；`npm run typecheck --prefix packages/react`、`npm run build --prefix packages/react && node packages/react/__test__/reconciler.cjs`、`node packages/react/__test__/text_pty.cjs` 已通过。下一步按顺序运行 `cargo fmt`、clippy、完整 Rust/Node/React 验证。
- 已通过 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`、`npm run typecheck --prefix packages/react`、`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`、`npm test --prefix packages/core`、`npm test --prefix packages/react`。下一步运行 napi build 与 `crates/atto-ui-node` npm 测试，再做 diff 检查并更新任务记录。
- 已通过 napi build、`crates/atto-ui-node` npm 测试与 `git diff --check`；未找到 `tools/run_fixtures.py`。已将 `TODO.md` 与 `TODO-1.md` 中 `NT12` 标记为 `[DONE]` 并写入完成记录；`PLAN.md` 未变更，因为阶段计划没有变化。
