# Claude 执行计划

## 范围

- 以 `TODO.md` 作为权威任务来源。
- 只完成第一个标题未带 `[DONE]` 的任务。
- 完成该任务的实现、验证、记录与提交后停止；如果需要新增前置/阻塞任务，则提交该任务清单调整后停止。

## 分步计划

1. 先读取 `TODO.md`，按标题前缀识别第一个未完成任务。
2. 阅读该任务的要求、依赖、验证要求和完成记录。
3. 仅围绕当前任务检查必要的 git 上下文，包括最新提交是否提到与该任务直接相关的未完成事项。
4. 检查当前任务相关源码和测试文件，确定最小正确实现路径。
5. 如果存在阻止正确实现的具体前置问题，则在 `TODO.md` 加入最少前置任务，提交该清单变更并停止。
6. 否则用小而聚焦的补丁完成实现。
7. 按要求运行验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行任务和仓库策略要求的相关/完整测试。
8. 对遇到的未排期失败测试或 fixture，要么修复，要么在标记完成前加入最少的前置/后续任务。
9. 更新 `TODO.md`，给完成任务标题加 `[DONE]` 并填写包含验证细节的完成记录。
10. 用清晰的任务相关提交信息提交所有相关变更。
11. 停止，不开始下一个任务。

## 进度记录

- 已在读取任务细节或运行项目命令前初始化计划文件。
- 已从 `TODO.md` 识别第一个未完成任务：`NT10`（`render()` + tick 主循环），完整细节位于 `TODO-1.md`。
- 已阅读 NT10 要求和当前 React/Node host 代码。实现路径为先加入显式 native 终端清理 hook，再实现 `packages/react/src/render.ts` 的非阻塞 `setImmediate` tick loop 与聚焦的 JS/PTY 测试。
- 已实现 native cleanup hook、React `render()` tick loop、包导出和 headless/PTY JS 测试。`cargo fmt`、clippy 和 TypeScript typecheck 已通过。
- 已完成 NT10 验证：Rust 完整测试、native build、Node/core/react JS 测试、TypeScript typecheck 和 diff whitespace 检查均通过。期间修复 PTY runner 终端尺寸设置，并让 `dispose()` 在读取终端尺寸失败时仍能恢复终端。已在 `TODO.md` 和 `TODO-1.md` 将 NT10 标为 `[DONE]` 并写入完成记录。
