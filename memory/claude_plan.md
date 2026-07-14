# 执行计划

## 目标

本次调用只完成 `TODO.md` 中第一个标题未带 `[DONE]` 前缀的任务，然后停止。`TODO.md` 是任务顺序、要求、依赖、验证和完成记录的唯一权威来源。

## 约束

- 全程使用中文记录进度和结果。
- 不做开放式历史问题扫描；先识别当前第一个未完成任务。
- 不跳过 review 任务或已有完成记录但标题未标 `[DONE]` 的任务。
- 遇到阻塞当前任务的缺陷、测试失败或规格不匹配时，优先修复；如果必须新增前置任务，则更新 `TODO.md`、提交并停止。
- 完成任务后必须更新 `TODO.md` 标题为 `[DONE]`，填写完成记录，运行要求的格式化、lint 和测试，再提交。
- 只在阶段级计划发生变化时更新 `PLAN.md`。

## 步骤

1. 读取 `TODO.md`，找到第一个标题未带 `[DONE]` 的任务，并记录任务范围、验证要求和依赖。
2. 查看最新提交信息，判断是否明确提到与当前任务直接相关的未完成事项。
3. 针对当前任务阅读必要代码和测试，避免无关范围扩展。
4. 实现当前任务；若发现必须先修复的阻塞问题，按规则修复或在 `TODO.md` 插入最小前置任务。
5. 运行 `cargo fmt`。
6. 运行 `cargo clippy --all-targets -- -D warnings`。
7. 运行完整测试套件，优先使用 `cargo test --all --all-targets`，并确保最长不超过 30 分钟。
8. 根据验证结果修复问题；如果出现未计划的失败测试，不能忽略，必须修复或加入 `TODO.md`。
9. 更新 `TODO.md`：把当前任务标题加 `[DONE]`，填写完成记录和验证结果。
10. 检查 git diff，提交所有与本次任务相关且当前未提交的变更。
11. 停止，不继续处理下一个任务。

## 当前状态

- 已读取 `TODO.md`。
- 当前第一个未完成任务：`M2-5 进程内语义 API：invoke / query / wait_for`。
- 已查看最新提交：`04ba857 [M2-4] Implement slider apply command`，未发现明确提到与 M2-5 直接相关的未完成事项。
- 已阅读 `src/inspect.rs`、`src/component_api.rs`、`src/composable/component.rs` 与现有 `apply_command` 实现。
- 设计调整：为满足“只有未实现才坐标兜底”，需要新增 `Component::supports_command(&ComponentCommand)`，否则无法区分默认 ignored 与禁用态有意 ignored。
- 已完成初版实现：新增可序列化的 invoke 结果/等待条件类型、`Component::supports_command`、`DesktopInspector::query` / `invoke` / `wait_for` / `wait_for_predicate`，并补进程内测试。
- 已运行 `cargo fmt --all`。
- 聚焦测试已通过：`cargo test -p atto-ui invoke_ -- --nocapture`、`cargo test -p atto-ui wait_for_ -- --nocapture`、`cargo test -p atto-ui query_matches -- --nocapture`。
- 已通过 `cargo fmt --all -- --check` 与 `git diff --check`。
- 已通过 `cargo clippy --workspace --all-targets -- -D warnings`。
- 已通过完整测试：`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 已更新 `TODO.md`：M2-5 标题已标记 `[DONE]`，完成记录和验证命令已写入。
- 下一步：检查最终 diff 并提交本次任务变更。

## M2-5 任务级计划

1. 阅读 `src/inspect.rs`、`src/component_api.rs`、`src/reactive/dirty.rs` 以及相关测试，确认现有 `action` / `action_target` / `get_property` / change tracker 的形状。
2. 设计并实现第 2 层 API：
   - `query(target, prop)` 作为 `get_property` 的统一命名入口。
   - `invoke(target, action)` 优先走组件 `apply_command` 语义派发；只有未实现或未消费时再走现有坐标兜底；返回结果必须暴露派发路径。
   - `wait_for(...)` 提供进程内等待能力，使用可序列化的目标、属性、期望值和 timeout 参数表达，并保留 predicate 闭包作为进程内便利 API。
3. 保持第 2 层不依赖第 3/4 层，不让不可序列化引用或闭包泄漏到稳定 API 边界；predicate 闭包只作为 M2 进程内例外。
4. 增加进程内单测：
   - `invoke("checkbox", Toggle)` 直接翻转 binding，并可观测到语义派发路径。
   - `query` 与 `get_property` 返回一致。
   - `wait_for` 能等到后台或定时驱动更新后的状态。
   - `wait_for` 超时返回错误且不会挂死。
5. 运行 `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
6. 更新 `TODO.md` 的 M2-5 标题为 `[DONE]` 并写入完成记录和验证命令。
7. 检查 diff 并提交本次任务所有变更，然后停止。
