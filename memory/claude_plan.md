# 当前执行计划

## 范围

- 以 `TODO.md` 作为唯一任务排序与完成状态来源。
- 仅处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 如遇阻塞当前任务的缺陷、缺失能力或测试/fixture 失败，优先修复；若无法在当前任务内正确修复，则在 `TODO.md` 中插入最小必要前置任务并提交后停止。

## 步骤

1. 读取 `TODO.md`，定位第一个未完成任务，并检查其任务正文、依赖、验证要求和完成记录。
2. 检查最新提交信息是否明确提到与该任务直接相关的未完成问题；如相关，将其纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 根据当前任务读取必要的代码、测试与文档上下文，避免无关的历史问题扫查。
4. 实现当前任务要求；如果发现必须先解决的具体阻塞项，更新 `TODO.md` 并停止在阻塞处理路径。
5. 运行格式化、lint 和相关测试；如代码有变更，按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行必要的完整测试。
6. 若发现未排期的失败测试或 fixture，修复或将最小必要修复任务排入 `TODO.md`，不得将当前任务标为完成。
7. 完成后更新 `TODO.md`：在任务标题前加 `[DONE]`，并填写完成记录与验证结果；仅在阶段计划真实变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交记录，确认仅提交本次任务相关变更，并用清晰提交信息提交。
9. 提交后停止，不继续处理下一个任务。

## 当前状态

- 已写入初始执行计划。
- 已读取 `TODO.md` 与 `TODO-1.md`；第一个未完成任务是 `NR7 — 审阅 NT7`。
- 最新提交为 `[NT7] Add RichText and TextSpan`，与当前审阅任务直接相关，无额外未完成前置说明。
- 已审阅 `src/text/styled_text.rs`、`src/widgets/rich_text.rs`、runtime 注册、schema 测试、快照 app 与 PTY 测试；未发现需要修改实现的当前任务阻塞问题。
- 已通过目标验证：`cargo test -p atto-ui rich_text`；`cargo test --test pty_rich_text`。
- 已通过完整验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。
- 未找到 `run_fixtures.py`，本仓库无单独 fixture 套件可运行。
- 已更新 `TODO.md` / `TODO-1.md` 的 `NR7` 完成状态和完成记录。
- 下一步：提交 `NR7` 审阅完成记录后停止。
