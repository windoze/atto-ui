# 执行计划

## 范围

- 以 `TODO.md` 为唯一任务顺序和完成状态来源。
- 本次只完成第一个未在标题中标记 `[DONE]` 的任务，然后停止。
- 如遇阻塞当前任务的真实缺口，只添加最小必要前置任务并提交，不绕过规格。

## 步骤

1. 读取 `TODO.md`，确定第一个未完成任务及其验证要求。
2. 检查最近提交信息是否提到与该任务直接相关的未完成问题。
3. 读取当前任务涉及的代码、测试和文档，确认最小正确改动范围。
4. 实现当前任务；如发现阻塞性规格缺口，更新 `TODO.md` 记录前置任务并停止。
5. 按要求运行格式化、lint 和相关测试；若有未安排的失败，修复或记录为正确顺序的任务。
6. 更新 `TODO.md`：在任务标题加 `[DONE]`，填写完成记录。
7. 仅在阶段计划实际变化时更新 `PLAN.md`。
8. 查看 git 状态和差异，提交本次任务相关全部变更。

## 当前状态

- 已读取 `TODO.md`，首个未完成任务为 `TODO-2.md` 中的 `T9`：L2 Code Action 请求、列表 popup 与单文档应用。
- 最近提交为 `462f174 [NT1] Update execution log`，未发现与 `T9` 直接相关的未完成事项。
- 已读取 `T9` 任务详情与 `atto-ui-editor` 现有 LSP、popup、输入、渲染和 mock LSP 测试结构。
- 实现方案：复用 `EditorView` 的 LSP poll；新增 `EditorAction::LspCodeAction` 和 `Ctrl+.` 默认键；新增 code action popup model/binding；响应 `textDocument/codeAction` 后填充 keyboard popup；Enter 使用 editor-core-lsp 的 code action apply plan，只应用当前 URI 的 WorkspaceEdit，跨 URI 通过 `EditorEvent` 明确提示跳过；命令动作走 `workspace/executeCommand`。
- 已完成代码实现和 mock LSP 集成测试补充；`cargo fmt` 与 `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- 完整测试套件 `cargo test --workspace --all-targets` 已通过。
- 已将 `TODO.md` 索引和 `TODO-2.md` 的 `T9` 标记为 `[DONE]`，并写入完成记录。
- 已检查 git status/diff，并通过 `git diff --check`；未纳入无关未跟踪文件 `notification.sh`、`run_agent.sh`。
- 已提交本任务相关变更，提交为 `41f4203 [T9] Add LSP code actions`。
- 本次调用到此停止，不继续处理 `R9`。
