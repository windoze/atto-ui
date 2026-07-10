# 执行计划

此文件记录本次调用的可公开执行计划与进度；不会记录私有推理过程。

## 当前目标

- 已从 `TODO.md` 识别第一个未完成任务：`M5.3 计划生成`。
- 任务要求：实现虚拟 tool `submit_plan({ items })`，并兜底解析 markdown 列表为 `PlanItem`。
- 本次只完成该任务，完成后提交并停止。

## 执行步骤

1. 检查最新提交是否声明与 `M5.3` 直接相关的未完成事项。
2. 定位现有 plan mode 判定、DeepSeek tool call 聚合、tool registry、Chat/Plan block 数据模型与测试。
3. 实现 `submit_plan` 虚拟工具的请求/聚合/解析路径，确保不会作为本地可执行工具绕过权限模型。
4. 实现 markdown 列表兜底解析为 `PlanItem`，覆盖常见有序/无序/checkbox 列表输入。
5. 增加或更新相关单元测试，必要时补充确定性 fixture 覆盖。
6. 按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、再运行相关或完整测试。
7. 更新 `TODO.md`，给 `M5.3` 标记 `[DONE]` 并写入完成记录。
8. 提交本次任务相关变更，停止。

## 进度记录

- 已记录初始计划。
- 已读取 `TODO.md`，确认当前任务为 `M5.3 计划生成`。
- 已检查最新提交 `[M5.2] Add auto plan detection`，未发现与 `M5.3` 直接相关的未完成事项。
- 已定位实现点：`plan.rs` 放置虚拟 `submit_plan` schema 与计划解析，`stream_ui.rs` 把流式结果映射为 plan action，`lib.rs` 负责把 `PlanBlock` 写入 store 并构建 plan draft request。
- 已完成核心实现：新增 `submit_plan` 虚拟工具 schema/forced tool choice、markdown fallback 解析、`PlanReady` action、plan draft request builder，以及 plan-mode mock stream。
- 已完成验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check` 均通过。
- 已更新 `TODO.md`，将 `M5.3 计划生成` 标记为 `[DONE]` 并写入完成记录。
- 已检查 `git diff --check`，未发现空白错误。
