# 执行计划

## 约束说明

- `TODO.md` 是任务顺序、要求、依赖、验证和完成记录的唯一权威来源。
- 每次只完成第一个标题未带 `[DONE]` 的任务，然后停止。
- 不做开放式历史问题排查；只处理会阻塞当前任务、使当前任务行为无效，或由当前任务引入的直接回归。
- 若遇到无法按规格完成的阻塞问题，最小化新增前置任务到 `TODO.md`，提交后停止。
- 完成任务前必须按要求验证：先格式化，再 lint，再运行相关或完整测试；未计划的失败测试不得忽略。
- 完成后更新 `TODO.md` 标题为 `[DONE]` 并填写完成记录，必要时更新 `PLAN.md`，然后提交。

## 步骤计划

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务，确认任务正文、依赖、验证要求和完成记录格式。
2. 检查最新提交信息，若其明确提到与当前任务直接相关的未完成问题，则将其纳入当前任务或作为前置任务记录。
3. 针对当前任务读取最小必要代码和文档上下文，避免无关范围扩张。
4. 若任务可直接实现，按最小正确改动完成实现；若存在具体阻塞，更新 `TODO.md` 增加最小前置任务并停止。
5. 根据任务要求补充或更新测试，保证覆盖新增行为和相关边界。
6. 运行 `cargo fmt`，随后运行 `cargo clippy --all-targets -- -D warnings`，再按要求运行相关测试或完整测试套件。
7. 若发现未被任务计划覆盖的失败测试，优先修复；若无法在当前任务内合理修复，则在 `TODO.md` 中添加正确顺序的前置任务并停止。
8. 任务完成后，将 `TODO.md` 中对应任务标题加上 `[DONE]`，更新完成记录，只有阶段级计划变化时才更新 `PLAN.md`。
9. 检查 `git status`、`git diff` 和最近提交，确认只提交意图内改动；提交本次任务相关变更。
10. 提交后停止，不继续下一个任务。

## 当前状态

- 已读取 `TODO.md`。
- 当前第一个未完成任务：`M6.5 Retry/Edit 重跑`。
- 任务要求：接入 `on_edit_and_resubmit`、retry/regenerate，截断后重启 agent turn。
- 已读取 `PLAN.md` 中 M6 设计上下文；阶段验收要求 PTY 覆盖 mention、compact、retry/edit 重跑，并确保取消后无迟到 token 污染新分支。
- 最新提交为 `[M6.4] Implement transcript compacting`，未声明与当前 M6.5 直接相关的未完成事项。
- 已检查 `atto-ui-chat` 的 edit/retry/regenerate 回调与 `atto-agent-app` 的 turn 启动、取消、compact 和 transcript 截断实现。
- 已实现 app 层 `on_edit_and_resubmit` 与 retry/regenerate `on_message_action` 接入：截断后取消当前后台 mock turn，复用普通 prompt 启动路径处理 skill、plan、compact，并重启 assistant turn。
- 已补充 app 单测覆盖编辑重发、retry/regenerate 从保留 user prompt 重启、旧 branch 迟到 token 被拒绝。
- 已运行 `cargo fmt --all` 和新增专项单测，均通过。
- 已运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过。
- 完整 `cargo test --workspace --all-targets` 发现 `agent_plan_mode_generates_plan_and_accept_continues_execution` 失败：启用 message actions 后消息高度增加，计划 accepted 标记被滚出 PTY 当前视口，等待 `[x] Accepted` 超时。
- 已调整该 PTY 用例为验证接受后插入的内部执行指令和后续 mock 执行，不再依赖已滚出视口的 accepted 标记；专项 PTY 用例已通过。
- 已重新运行 `cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 和 `cargo fmt --all -- --check`，均通过。
- 已更新 `TODO.md`：`M6.5 Retry/Edit 重跑` 已标记 `[DONE]` 并写入完成记录与验证命令。
- 已检查 `git status`、`git diff` 和最近提交；当前仅有本任务相关文件变更。
- 已提交本次 M6.5 相关改动：`a7d97f9 [M6.5] Implement retry and edit rerun`。
- 下一步：停止，不继续处理下一个 TODO 任务。
