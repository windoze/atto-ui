# 当前执行计划

## 目标

按照 `TODO.md` 的任务顺序，仅完成第一个未标记 `[DONE]` 的任务；完成后更新任务记录、验证、提交并停止。

## 执行步骤

1. 读取 `TODO.md`，识别第一个标题未带 `[DONE]` 的任务，并确认其要求、依赖和验证方式。
2. 查看最近提交与当前工作区状态，判断是否存在与当前任务直接相关的未完成事项或未提交改动。
3. 读取当前任务涉及的代码、测试和文档，确定最小正确实现范围。
4. 按任务要求实现或修复；如发现阻塞当前任务的真实前置问题，先在 `TODO.md` 插入最小前置任务并停止。
5. 运行格式化、lint 和相关测试；若代码变更影响广泛，再运行完整测试套件。
6. 在 `TODO.md` 中把完成任务标题加上 `[DONE]`，并补充完成记录；仅当阶段计划确实变化时才更新 `PLAN.md`。
7. 检查 `git status`、`git diff`、最近提交，确认只提交本次任务相关改动。
8. 使用清晰的任务提交信息提交本次改动，然后停止，不继续处理后续任务。

## 进度记录

- 已创建初始执行计划，下一步读取 `TODO.md` 识别当前任务。
- 已读取 `TODO.md`，确认本轮唯一任务为 `M6.R Review`：复核上下文预算、分支 token、长会话性能和全套验证。
- 已查看工作区状态和最近提交：仅 `memory/claude_plan.md` 为本轮新增改动；最近提交为 `[M6.8] Add agent PTY coverage`，未发现直接阻塞本 review 的提交说明。
- 下一步复核 M6 相关实现和测试覆盖，重点包括 `ContextBuilder`、file mention、tool output budget、compact、retry/edit、transcript 持久化、状态栏和 PTY 覆盖。
- 复核发现与 `M6.R` 直接相关的阻塞项：compact/token 估算未覆盖 file mention 展开后的真实模型上下文；错误 mention 和 wrapper 开销未纳入总预算；启用 transcript 持久化时流式 delta 可能频繁同步全量保存。当前计划调整为先修复这些阻塞项，再继续验证和标记 review 完成。
- 已实施修复：file mention 注入增加硬总预算、数量上限和严格 UTF-8 校验；compact/status token 估算保守计入 mention 展开预算；compact 摘录对超大输出做限量扫描；transcript 持久化合并脏保存并在退出/显式保存时强制落盘。下一步运行格式化和针对性测试。
- 验证已通过；在最后清理 mention 解析上限后已重新运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。已将 `TODO.md` 中 `M6.R Review` 标记为 `[DONE]` 并补充完成记录。
- 下一步检查 diff、确认只包含本轮任务相关改动，然后提交并停止。
