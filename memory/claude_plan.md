# 执行计划

此文件记录本次调用的可公开执行计划与进度；不会记录私有推理过程。

## 当前目标

- 已从 `TODO.md` 识别第一个未完成任务：`M5.4 PlanBlock UI`。
- 任务要求：渲染 `PlanBlock { decision: Pending }`，接入 `on_plan_decision`。
- 本次只完成该任务，完成后提交并停止。

## 执行步骤

1. 检查最新提交是否声明与 `M5.4` 直接相关的未完成事项。
2. 定位 `PlanBlock` 数据模型、当前 chat block 渲染代码、输入/列表事件回调和已有测试。
3. 设计并实现 pending plan 的可见 UI，包括计划条目、待决状态和可操作提示，保持现有视觉风格。
4. 将 UI 决策事件接到现有 `on_plan_decision`/事件分发路径，确保 accept/reject 操作能到达 app 层；实际 accept/reject 流程若属于 `M5.5`，本任务只完成事件接入和状态传递。
5. 增加或更新相关单元测试或 PTY/snapshot 覆盖，验证 pending `PlanBlock` 可渲染且决策回调可触发。
6. 按要求运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、再运行相关或完整测试。
7. 更新 `TODO.md`，给 `M5.4` 标记 `[DONE]` 并写入完成记录。
8. 提交本次任务相关变更，停止。

## 进度记录

- 已记录初始计划。
- 已读取 `TODO.md`，确认当前任务为 `M5.4 PlanBlock UI`。
- 已检查最新提交 `[M5.3] Add plan generation`，未发现与 `M5.4` 直接相关的未完成事项。
- 已定位现有 `atto-ui-chat` 的 `PlanDecisionView`、`PlanBlock` 渲染和事件发射实现；缺口在 app 构建 `ChatMessageList` 时未绑定 `on_plan_decision`。
- 已完成 app 层接入初稿：新增 `handle_plan_decision`，在 `build_chat_panel` 绑定 `.on_plan_decision`，将 pending `PlanBlock` 锁定为 accepted/rejected，并拒绝已决 plan 的旧事件覆盖。
- 已补充 app 单测 `plan_decision_callback_updates_pending_plan_block`，覆盖决策状态更新和重复旧事件忽略。
- 已完成验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check` 均通过。
- 已更新 `TODO.md`，将 `M5.4 PlanBlock UI` 标记为 `[DONE]` 并写入完成记录。
