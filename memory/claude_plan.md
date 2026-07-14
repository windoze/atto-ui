# 执行计划

## 约束

- 本轮只完成 `TODO.md` 中第一个未标记 `[DONE]` 的任务，然后停止。
- `TODO.md` 是任务顺序、完成状态、依赖和验收要求的唯一依据。
- 在识别任务前不做开放式历史问题扫描。
- 如果发现阻塞当前任务的缺陷或未排期失败测试，先修复，或在 `TODO.md` 中插入最小必要前置任务并停止。
- 任何完成的任务必须在标题中显式加上 `[DONE]`，仅填写完成记录不算完成。
- 常规任务执行只更新 `TODO.md`；只有阶段级计划、依赖或完成标准改变时才更新 `PLAN.md`。
- 代码改动后按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试套件，完整测试超时不超过 30 分钟。
- 提交时包含本轮任务相关的所有未提交改动；如果是恢复未完成任务，也将当前未提交文件一并纳入提交。

## 初始计划

1. 读取 `TODO.md`，按文件顺序定位第一个标题未带 `[DONE]` 的任务。
2. 检查最新提交信息；只有当它明确提到与当前任务直接相关的未完成问题时，才把该问题纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 阅读当前任务相关的代码、测试和文档，确认验收标准、依赖和影响范围。
4. 实施当前任务；如遇到阻塞性规格不匹配、缺失能力或失败测试，优先处理，不能绕过。
5. 更新或新增针对当前任务的测试，避免只覆盖过窄路径。
6. 运行格式化、lint 和测试验证；若失败，修复后重复相关验证。
7. 在 `TODO.md` 中给当前任务标题加 `[DONE]`，并更新完成记录，写明实现要点和验证命令。
8. 如果阶段级计划没有变化，不更新 `PLAN.md`。
9. 检查 git 状态，提交本轮相关改动，提交信息包含任务编号和简明说明。
10. 停止，不继续处理后续任务。

## 进度记录

- 已创建本执行计划文件。下一步读取 `TODO.md` 识别第一个未完成任务。
- 已读取 `TODO.md`，本轮任务确定为 `M1-3 tag 覆盖诊断辅助`。
- 已检查最新提交 `471feae [M1-2] Add DesktopInspector property names`，未发现与 `M1-3` 直接相关的未完成提交说明。
- 下一步阅读 `src/inspect.rs`、`src/composable/component.rs` 和相关测试，确定 `InspectNode` 结构、快照树生成流程与可交互节点判定实现位置。
- 已完成相关代码阅读：`InspectNode` 已包含 id/properties/tree 上下文，`Component` 提供 `is_focusable()`，因此诊断实现可放在 `src/inspect.rs` 内。
- 已在 `DesktopInspector` 上新增 `untagged_interactive_nodes(screen)`，并让 `InspectNode` 携带 `focusable` 诊断字段；树构建时填充组件/窗口的可聚焦状态。
- 已新增过滤 helper 和单测：构造带 tag Checkbox 与未带 tag Checkbox 的 Desktop，断言诊断只返回未带 tag 的 Checkbox。
- 已运行 `cargo fmt --all`。
- 已运行聚焦测试 `cargo test -p atto-ui untagged_interactive_nodes -- --nocapture`，新增诊断测试通过。
- 已运行 `cargo fmt --all -- --check`，通过。
- 已运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过。
- 已运行完整测试 `cargo test --workspace --all-targets`（30 分钟超时），通过。
- 已更新 `TODO.md`：`M1-3` 已标记 `[DONE]`，并补充完成记录与验证命令。
- 下一步检查 diff / git status，确认改动范围后提交本轮任务。
