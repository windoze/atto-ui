# 执行计划

> 说明：这里记录可审计的执行计划、关键决策和进度更新，不记录逐字内部推理。

## 当前目标

- 当前任务：`P1.R Review：P1 阶段复核`。
- 复核 P1.0-P1.3 的高亮选型、代码块高亮、diff 高亮、快照与 PTY 覆盖；确认边界场景无问题并跑通 CI 命令。
- 完成后更新 `TODO.md` 的 P1.R 标题和完成记录，提交 Git commit，然后停止。

## 步骤

1. 读取 `TODO.md`，只识别第一个未完成任务，不做开放式历史问题扫描。
2. 检查最新提交信息；仅当它明确提到与当前任务直接相关的未完成问题时，把它纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 阅读当前任务要求、依赖、验证标准，以及必要的相关代码。
4. 若任务可直接完成，进行最小正确实现；若发现阻塞当前任务的具体前置问题，则按要求更新 `TODO.md` 并停止。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行必要测试；若需要完整测试，使用足够长的超时。
6. 修复验证中暴露且未被明确排期的问题；若无法在当前任务内修复，添加最小前置/后续任务并保持当前任务未完成。
7. 在 `TODO.md` 中给完成任务标题加 `[DONE]`，并更新完成记录；仅在阶段计划真实变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、`git log --oneline -10`，确认只提交本次相关变更。
9. 创建清晰的 Git commit，然后停止，不继续下一个任务。

## 进度

- 已写入初始计划。下一步：读取 `TODO.md` 并确定第一个未完成任务。
- 已读取 `TODO.md`：第一个未完成任务为 `P1.R Review：P1 阶段复核`。
- 最新提交 `ae668a5 [P1.3] Add syntax highlight PTY snapshots` 未显示与 P1.R 直接相关的未完成事项。
- 下一步：阅读 P1 相关实现和测试，核对 no-unsafe/fallback/diff 语义/宽字符/长代码块/未闭合代码块/PTY 覆盖。
- 已完成实现复核：`atto-ui-markdown` 使用 `syntect` 且关闭默认特性，启用 `default-syntaxes` + `regex-fancy`；高亮接口隐藏 syntect 类型；未知语言回退纯文本；代码块保留 plain lines 用于宽度和滚动；diff payload 高亮在增删行上保留语义前景/背景；新增 markdown/chat PTY 覆盖存在。
- 依赖核对：`cargo tree -p atto-ui-markdown -e normal` 显示 syntect 分支使用 `fancy-regex`，未引入 onig；`Cargo.lock` 中的 onig 不是该 crate 正常依赖树的一部分。
- 下一步：运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 验证完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 下一步：更新 `TODO.md`，将 P1.R 标记为 `[DONE]` 并记录完成情况。
- 已更新 `TODO.md`：P1.R 标题已加 `[DONE]` 并填写完成记录。
- 下一步：检查 `git status`、`git diff`、`git log --oneline -10`，确认后提交本次 review 任务。
- 已检查 `git status --short`：仅 `TODO.md` 与 `memory/claude_plan.md` 有变更。
- 已检查 `git diff -- TODO.md memory/claude_plan.md`：变更范围为 P1.R 完成记录与本计划文件。
- 已检查 `git log --oneline -10`：最近提交为 `ae668a5 [P1.3] Add syntax highlight PTY snapshots`。
- 下一步：重新确认最终 diff 后提交。
