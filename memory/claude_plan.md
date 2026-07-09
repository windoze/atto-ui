# 执行计划

## 范围
- 以 `TODO.md` 为唯一任务顺序和完成状态来源。
- 本次只识别并完成第一个标题未带 `[DONE]` 的任务，然后停止。
- 若发现当前任务被具体前置问题阻塞，将在 `TODO.md` 中插入最小必要前置任务并提交后停止。

## 步骤
1. 读取 `TODO.md`，确定第一个未完成任务及其验证要求。
2. 检查最新提交信息，只判断是否存在与该任务直接相关的未完成事项。
3. 阅读当前任务涉及的代码、测试和文档，限定在任务所需范围内建立上下文。
4. 实现任务要求，保持改动最小且符合现有结构。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行必要测试；若代码变更需要完整验证，则运行完整测试套件。
6. 处理所有未被显式排期的测试失败；不能绕过或忽略失败。
7. 更新 `TODO.md`：在任务标题前加 `[DONE]`，填写完成记录和验证结果。
8. 仅在阶段计划实际变化时更新 `PLAN.md`。
9. 检查 `git status`、`git diff`、最近提交，确认只提交本任务相关改动。
10. 使用清晰提交信息提交全部相关改动，然后停止。

## 进度记录
- 初始计划已写入，下一步读取 `TODO.md` 确定当前任务。
- 已读取 `TODO.md`，首个未完成任务是 `M2.5 错误映射`。
- 下一步检查最新提交是否有与 `M2.5` 直接相关的未完成事项，并阅读 DeepSeek 流式映射、错误类型和 UI 错误展示相关代码。
- 最新提交为 `[M2.4] Add DeepSeek stream UI mapping`，未发现提交信息中有与 `M2.5` 直接相关的未完成事项。
- 当前实现步骤：定位 `ChatErrorKind`、DeepSeek SSE 错误事件、stream UI mapper 与 app action handler，补齐错误到 UI 的明确映射和测试。
- 已实现 `M2.5` 主要代码：新增 failed turn action、DeepSeek 错误映射、失败回合 branch 失效语义，并补充 app/store 单测。
- 下一步运行 `cargo fmt`，随后按要求运行 clippy 和测试；若发现失败或警告，先修复再更新 `TODO.md`。
- 验证已完成并通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 下一步更新 `TODO.md`，将 `M2.5` 标记为 `[DONE]` 并记录实现与验证结果，然后检查 diff 并提交。
- `TODO.md` 已将 `M2.5` 标记为 `[DONE]` 并记录完成内容与验证命令；`git diff --check` 通过。
- 下一步提交本次任务相关改动并停止。
