# 当前执行计划

## 范围

本轮只完成 `TODO.md` 中第一个未完成任务，完成验证、任务记录和 git 提交后停止。

## 步骤

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 确认第一个未完成任务。
2. 仅检查与当前任务直接相关的最新提交未完成事项。
3. 阅读当前任务要求、依赖、完成标准和验证要求。
4. 实施满足任务的最小完整变更，不改变无关行为。
5. 先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，再运行相关或完整测试。
6. 若验证暴露未排期失败或阻塞性规格不匹配，优先修复；否则在 `TODO.md` 插入最小前置任务并停止。
7. 完成后在 `TODO.md` 给任务标题加 `[DONE]` 并填写完成记录。
8. 提交本任务相关全部变更，停止，不继续下一个任务。

## 进度

- 初始计划已在仓库检查前记录。
- 已选择第一个未完成任务：`M6.8 快照与测试`。
- 覆盖要求：PTY 覆盖 file mention、compact、retry/edit 重跑、取消后迟到 token 不显示。
- 已实现 snapshot 专用 compact 小阈值、基于 `deepseek_request_from_transcript` 的 mock context probe，以及 mention、compact、retry/edit 的新 PTY 用例；既有 Esc 取消 PTY 用例继续覆盖迟到 token 拒绝。
- 针对性验证已通过：`cargo test -p atto-agent-app --test pty_agent`。
- 完整验证已通过：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已在 `TODO.md` 将 `M6.8` 标记为 `[DONE]`，并写入完成记录和验证记录。
