# Claude 执行计划

## 范围
- 以 `TODO.md` 作为权威任务来源。
- 只完成第一个标题未带 `[DONE]` 的任务。
- 完成当前任务的审阅、必要修复、验证、记录与提交后停止；如遇阻塞，则只记录最小前置任务并提交后停止。

## 分步计划
1. 读取 `TODO.md`，按顺序识别第一个未完成任务。
2. 仅检查最新提交中与该任务直接相关的未完成事项。
3. 阅读任务来源文件中的完整要求、依赖与验证要求。
4. 围绕当前任务检查必要源码和测试，避免无关历史问题扫查。
5. 对审阅发现的真实缺口做最小正确修复或补充测试，不用 workaround 代替规格行为。
6. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、Rust 完整测试，以及相关 TS/JS/PTY 验证。
7. 若发现未排期失败测试或 fixture，要么修复，要么在 `TODO.md` 中加入最小前置/后续任务后停止。
8. 将当前任务标题标记为 `[DONE]`，写入完成记录和验证结果。
9. 检查 `git status`、`git diff`、`git log --oneline -10`，只暂存本任务相关文件并提交。
10. 提交后停止，不开始下一个任务。

## 进度记录
- 已记录初始执行计划。
- 已从 `TODO.md` 识别首个未完成任务：`NR10`（审阅 `NT10`）。
- 最新提交为 `[NT10] Add React render tick loop`，与 `NR10` 直接相关。
- 审阅范围确定为 `packages/react/src/render.ts`、Node `AppHost.dispose()`、Rust `AppHost::restore_terminal()` 与 React render PTY 测试。
- 已补充 `NR10` 审阅覆盖：timer 驱动的 React 更新、鼠标捕获恢复序列、PTY raw mode flags 恢复。
- 已完成验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`、Node native build、Node/core/react package typecheck 与测试、`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。
- 已在 `TODO-1.md` 将 `NR10` 标记为 `[DONE]`，并同步更新 `TODO.md` 索引。
- 最终小幅调整 React render 测试后，已重新运行 `npm test --prefix packages/react` 与 `git diff --check`。
