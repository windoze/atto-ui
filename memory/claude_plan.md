# 执行计划

## 当前约束
- 以 `TODO.md` 为唯一任务顺序和完成状态来源。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 完成任务后需要更新 `TODO.md`、运行要求的格式化/检查/测试，并提交 Git commit。
- 若遇到阻塞当前任务的缺陷或未排期测试失败，先修复或在 `TODO.md` 中加入最小前置任务并停止。

## 步骤计划
1. 读取 `TODO.md`，定位第一个未完成任务及其验收要求。
2. 检查最近提交是否明确提到与该任务直接相关的未完成问题。
3. 阅读与当前任务相关的代码、测试和文档，确认实现范围。
4. 进行最小且完整的实现，不绕过规格要求。
5. 按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试套件；若任务仅改文档且已有可复用绿色结果，则按规则跳过完整测试并记录原因。
6. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，补充完成记录、验证命令和结果。
7. 检查 Git 状态和 diff，提交本任务相关的全部未提交更改。
8. 停止，不处理下一个任务。

## 进度记录
- 已创建本计划文件，下一步读取 `TODO.md` 确认当前任务。
- 已确认第一个未完成任务为 `M1.5 取消语义`；最近提交 `[M1.4] Add slash commands` 未声明与该任务直接相关的未完成事项。
- 下一步阅读 app crate、chat 组件和事件处理代码，定位 `on_cancel`、Esc 以及 mock turn 的取消边界。
- 已确认实现范围：`ChatMessageList::on_cancel` 已存在但 app 未接入；`/abort` 只取消 UI 状态，尚未终止后台 mock 线程。
- 当前编辑计划：新增会推进 branch token 的 store 取消方法；app 维护当前 mock turn 取消令牌；`/abort`、列表 `on_cancel` 和输入 Esc 共用同一取消路径。
- 已完成核心代码编辑：`cancel_streaming_turn` 会取消 streaming turn 并推进 branch token；app 的 `/abort`、`/clear`、list `on_cancel`/Esc 都会取消当前 mock token 并复位 UI 状态。
- 下一步运行 `cargo fmt`，随后根据编译/lint 结果修正问题。
- 首次 `cargo clippy --workspace --all-targets -- -D warnings` 发现 app 测试缺少显式 `crossterm` dev-dependency，以及 `with_runtime_state` 参数过多；已通过新增 dev-dependency 和 `AgentRuntime` 结构修复。
- `cargo fmt` 已通过；第二次 `cargo clippy --workspace --all-targets -- -D warnings` 已通过。正在运行完整测试套件。
- 调整 store 取消测试后重新验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check` 均已通过。
- `TODO.md` 已标记 `M1.5` 为 `[DONE]` 并写入完成记录；下一步检查最终 diff 并提交。
