# 当前调用计划

## 范围

- 以 `TODO.md` 作为任务顺序与完成状态的权威来源。
- 本次只完成第一个标题未标记 `[DONE]` 的任务，然后停止。
- 如发现阻塞当前任务的真实缺口，只做当前任务所需的最小正确修复，不绕过规格。

## 执行步骤

1. 读取 `TODO.md`，确定第一个未完成任务及验证要求。
2. 只检查与当前任务直接相关的最近提交背景。
3. 审阅当前任务涉及的最小代码、测试与任务记录。
4. 如发现审阅问题，修复并补充覆盖；否则只更新完成记录。
5. 依次运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
6. 将完成任务标题显式改为 `[DONE]`，并同步 `TODO.md` 索引。
7. 检查 git 状态、差异和最近提交，只暂存本次任务相关文件。
8. 提交本次任务并停止，不继续下一个任务。

## 进度记录

- 已初始化计划并读取任务清单。
- 已选定首个未完成任务：`R9 — 审阅 T9`。
- 审阅范围：T9 的 code action 请求/响应、popup keyboard 分发、单文档 WorkspaceEdit 应用、跨文件 edit 跳过、command 执行、LSP didChange 与 syntax refresh。
- 审阅结论：跨文件 edit 拒绝、popup 优先级、单文档 edit 后同步路径未发现代码缺陷；但 command-only code action 缺少可观测回归测试。
- 已通过 `mock_lsp_server` 与 `tests/lsp_editor.rs` 补充 command-only code action 集成测试，确认无 edit 时仍发送 `workspace/executeCommand`。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
- 已将 `TODO-2.md` 中 `R9` 标记为 `[DONE]`，并同步 `TODO.md` 索引。
- 已检查 `git diff --check`，未发现 whitespace 错误；未纳入无关未跟踪文件 `notification.sh`、`run_agent.sh`。
