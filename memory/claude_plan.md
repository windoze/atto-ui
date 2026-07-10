# 执行计划

说明：本文件记录可审计的执行计划、关键决策和执行进度；不会记录私密推理链路。

## 当前目标

- 按 `TODO.md` 的顺序完成第一个标题未带 `[DONE]` 的任务。
- 完成后更新 `TODO.md` 的任务标题和完成记录。
- 按要求运行格式化、lint 和相关测试；若观察到未排期的失败测试，先修复或在 `TODO.md` 中加入最小必要前置任务。
- 提交本次任务相关的所有变更，然后停止。

## 步骤

1. 读取 `TODO.md`，识别第一个未完成任务及其验证要求。
2. 查看最近提交信息，仅在其明确提到与当前任务直接相关的未完成问题时纳入当前任务或添加前置任务。
3. 读取当前任务涉及的代码、测试和文档，确认实现边界。
4. 以最小正确改动实现任务；若发现阻塞性规格不匹配，更新 `TODO.md` 添加前置任务并停止。
5. 运行 `cargo fmt`。
6. 运行 `cargo clippy --all-targets -- -D warnings`。
7. 运行任务要求的测试；若需要完整验证，运行 `cargo test --all --all-targets` 并设置足够超时。
8. 将任务标题标记为 `[DONE]`，更新完成记录和本计划文件。
9. 检查 `git status`、`git diff` 和最近提交，确认只提交本次相关变更。
10. 创建清晰的 Git 提交并停止，不继续下一个任务。

## 进度

- 已创建初始执行计划。
- 已识别首个未完成任务：`M6.2 文件 mention`。最近提交 `M6.1 Implement ContextBuilder` 未声明与该任务直接相关的未完成事项。
- 下一步读取 M6 设计、ContextBuilder、DeepSeek request 构建和现有文件/路径工具实现，复用已有 workspace 安全边界。
- 已在 `ContextBuilder` 中补齐 `@path` 解析、workspace 内文件读取、UTF-8 安全截断、单文件 32 KiB / 总计 128 KiB 预算和 `<context_files>` 注入。
- 已将普通 DeepSeek request 和 plan request 构建入口接入 `config.workspace` 的 file mention expansion。
- 已新增单测覆盖正常注入、越界/缺失文件错误记录、预算限制和 request 构建入口。
- 已完成验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-agent-app context_builder`、`cargo test -p atto-agent-app deepseek_request_from_transcript_injects_file_mentions_from_config_workspace`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已将 `TODO.md` 中 `M6.2 文件 mention` 标记为 `[DONE]` 并补充完成记录。
- 下一步检查 git diff/status，确认提交本次任务相关文件。
