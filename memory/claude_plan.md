# 执行计划

## 约束
- 以 `TODO.md` 为任务顺序和完成状态的唯一来源。
- 本次只完成第一个标题未带 `[DONE]` 的任务，完成后停止。
- 不做开放式历史问题清扫；只处理阻塞当前任务或测试失败策略要求处理的问题。
- 若发现无法按原任务完成的具体前置问题，更新 `TODO.md` 并提交后停止。
- 只在阶段级计划变化时更新 `PLAN.md`。

## 步骤
1. 读取 `TODO.md`，识别第一个未完成任务及其验证要求。
2. 查看最近提交和当前工作区状态，判断是否有与该任务直接相关的未完成事项或未提交变更。
3. 读取任务相关代码、测试和文档，确定最小正确实现范围。
4. 实现当前任务；若遇到阻塞当前任务的规格缺口或测试失败，优先修复或把最小前置任务插入 `TODO.md`。
5. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后按任务要求运行相关或完整测试；完整测试使用足够长的超时。
6. 更新 `TODO.md`：在任务标题前加 `[DONE]` 并补全完成记录；仅在阶段计划变化时更新 `PLAN.md`。
7. 检查 `git status`、`git diff`、`git log --oneline -10`，确认只提交本任务相关文件。
8. 使用包含任务编号的清晰提交信息提交变更，然后停止。

## 进度日志
- 已创建初始执行计划，下一步读取 `TODO.md` 识别首个未完成任务。
- 已读取 `TODO.md`，首个未完成任务为 `M2.2 DeepSeek 请求模型`。本次只处理该任务：定义 DeepSeek/OpenAI-compatible request/response/SSE 数据结构，并构造 `/chat/completions` 请求。
- 已确认最近提交为 M2.1 配置加载且无直接未完成事项；工作区仅有本计划文件变更。实现将集中在 `crates/atto-agent-app`：新增 `deepseek` 协议模块、导出模块并补充单测，不引入网络 client。
- 已新增 `crates/atto-agent-app/src/deepseek.rs`，包含 chat completions request/response/SSE chunk/error/tool choice/tool call 数据结构、endpoint 拼接和请求构造测试；已在 app crate 加入 `serde_json` 并导出模块。
- 首次完整测试发现一个测试断言使用 `0.4` 导致 `f32` JSON 精度不一致；已改为可精确表示的 `0.5`，准备重新运行验证。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。下一步更新 `TODO.md` 完成记录并提交。
- 已将 `TODO.md` 中 `M2.2 DeepSeek 请求模型` 标记为 `[DONE]` 并填写完成记录。下一步提交本任务变更后停止。
