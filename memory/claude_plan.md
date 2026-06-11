# 执行计划

> 说明：这里记录可审计的执行计划、决策依据和进度，不记录私有推理链。

## 当前目标

- 按 `TODO.md` 的权威顺序完成第一个标题未带 `[DONE]` 的任务。
- 完成该任务后更新 `TODO.md` 的完成记录，提交 Git commit，并停止。

## 初始步骤

1. 读取 `TODO.md`，只定位第一个未完成任务，不做开放式历史问题扫描。
2. 查看最近提交信息，只有在其明确提到与当前任务直接相关的未完成问题时才纳入当前任务或作为前置任务写入 `TODO.md`。
3. 阅读当前任务涉及的源码、测试和文档，确认任务要求、依赖和验证方式。
4. 若发现当前任务被缺失功能、规格不匹配或未安排的失败测试阻塞，按要求在 `TODO.md` 中新增最小前置任务并提交后停止。

## 实施步骤

1. 以最小正确改动实现当前任务，不绕开规格要求。
2. 为新增或变更行为补充必要测试。
3. 分阶段运行格式化、lint 和测试：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后运行相关或完整测试套件。
4. 如验证失败，修复根因；未安排的失败测试不能忽略。

## 收尾步骤

1. 将当前任务标题前缀更新为 `[DONE]`，并填写完成记录和验证结果。
2. 仅在阶段计划本身变化时更新 `PLAN.md`。
3. 检查 `git status`、`git diff`、最近提交记录，确认仅提交本次相关变更。
4. 提交 Git commit，提交信息包含任务编号和简要说明。
5. 停止，不继续下一个任务。

## 进度日志

- 已创建本计划文件，下一步读取 `TODO.md` 定位首个未完成任务。
- 已读取 `TODO.md`：首个未完成任务为 `P1.1 新消息模型`。
- 已查看最近提交：`3623f2b Add agent chat UI design doc and plan; archive UI-gaps docs`，未声明与 `P1.1` 直接相关的未完成事项。
- 下一步读取 `CHAT_UI.md` §3 以及 `src/message.rs`/相关引用，实施新消息模型。
- 已确认影响范围：`message.rs`、`lib.rs`、`list.rs`、`store.rs`、`dynamic.rs`、chat 示例/快照，以及 editor 快照中对旧 `ChatSender` 等类型的引用。
- 实施策略：完成 `P1.1` 的新消息 envelope、`ChatRole`、`ChatBlockId`、`ChatTurnStatus`、错误/meta/usage/stop reason；为现有单内容渲染和旧 store 方法添加过渡适配，完整内容块类型矩阵留给后续 `P1.2`。
- 已完成 Rust 侧首轮改造并通过 `cargo check -p atto-ui-chat --all-targets`。下一步运行格式化、lint 和测试。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 下一步更新 `TODO.md` 的 `P1.1` 标题和完成记录，然后检查 diff 并提交。
- 已更新 `TODO.md`：`P1.1` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 下一步检查工作区状态/diff/最近提交，确认提交范围后创建任务提交。
- 已检查提交前状态：本次提交将包含 `P1.1` 相关 Rust/TODO/计划文件，并按用户要求包含已变更的 `PROMPT.md`；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
